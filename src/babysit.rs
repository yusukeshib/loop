//! Wrapper over the `babysit` worker fleet.
//!
//! Step 3+4: every data + control path that CAN run in-process now does. `list`,
//! `prune`, `status_exists`, `kill`, `flag`, `unflag` and `attach` call the
//! babysit LIBRARY directly (no subprocess, no JSON re-parse). The ONE verb that
//! must still shell out is detached spawn (`start-session` → `babysit run -d`):
//! babysit's detacher re-execs `std::env::current_exe()` as the supervisor, which
//! in-process would be `looop` (it has no `run --detached-id`), so the real
//! `babysit` binary must own that fork. The leading `::babysit` disambiguates the
//! extern crate from this same-named module.

use serde::Deserialize;
use std::sync::OnceLock;

/// A process-wide current-thread tokio runtime to drive babysit's async API.
/// looop is otherwise synchronous; async is confined to this boundary.
fn rt() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        // enable_all: `attach` drives a control socket + PTY (needs the IO and
        // time drivers). The list/prune paths only touch the filesystem, but a
        // single shared runtime keeps the async boundary in one place.
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("looop: failed to build tokio runtime")
    })
}

/// One row of `babysit ls --json`. Tolerant of missing fields (a starting
/// session may not have an exit code or note yet).
#[derive(Debug, Deserialize, Default)]
pub struct Session {
    pub id: String,
    // `cmd` is an array of argv strings in `babysit ls --json`; keep it as a raw
    // Value so a type mismatch can never fail the WHOLE list parse (it is not
    // consumed here anyway). Tolerance over precision for an external schema.
    #[serde(default)]
    #[allow(dead_code)]
    pub cmd: Option<serde_json::Value>,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub alive: bool,
    #[serde(default)]
    pub exit_code: Option<i64>,
    #[serde(default)]
    pub note: Option<String>,
}

impl Session {
    pub fn is_looop(&self) -> bool {
        self.id.starts_with("looop-")
    }
    /// True when the worker has raised a flag (a non-empty note).
    pub fn flagged(&self) -> bool {
        self.note.as_deref().map(|n| !n.is_empty()).unwrap_or(false)
    }
}

/// List sessions via the babysit library, IN-PROCESS. Any failure yields an
/// empty list (matches the bash `2>/dev/null || true`): the pulse degrades
/// gracefully, never wedges. babysit's `paths::root()` reads `$BABYSIT_DIR`
/// fresh, so this honors looop's profile scoping just like the old shell-out.
pub fn list() -> Vec<Session> {
    match rt().block_on(::babysit::api::list_sessions()) {
        Ok(sessions) => sessions
            .into_iter()
            .map(|s| Session {
                id: s.id,
                cmd: None,
                state: s.state,
                alive: s.alive,
                exit_code: s.exit_code.map(|c| c as i64),
                note: s.note,
            })
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// looop-owned sessions only.
pub fn list_looop() -> Vec<Session> {
    list().into_iter().filter(Session::is_looop).collect()
}

/// Clear exited/dead-owner corpses, IN-PROCESS and SILENTLY. babysit's own
/// `sub::prune` prints to stdout (it is a CLI handler), which would pollute
/// looop's pretty output, so we drive the same decision over babysit's lower
/// level `session`/`paths` modules instead. Best-effort: any error is ignored,
/// matching the old `2>/dev/null` shell-out.
pub fn prune() {
    use ::babysit::session::{self, State};
    rt().block_on(async {
        let ids = match session::list_ids().await {
            Ok(ids) => ids,
            Err(_) => return,
        };
        for id in ids {
            let Ok(meta) = session::read_meta(&id).await else {
                continue; // unparseable meta — leave it alone, never nuke blind
            };
            let status = session::read_status(&id).await.ok();
            let alive = session::is_pid_alive(meta.babysit_pid);
            let dead = match status.as_ref().map(|s| s.state) {
                Some(State::Exited | State::Killed) => true,
                Some(State::Starting | State::Running) if !alive => true,
                None if !alive => true,
                _ => false,
            };
            if dead && let Ok(dir) = ::babysit::paths::session_dir(&id) {
                let _ = tokio::fs::remove_dir_all(&dir).await;
            }
        }
    });
}

/// Does a session with this id exist? In-process: the id is present in the
/// fleet list (replaces `babysit status -s <id>`).
pub fn status_exists(session: &str) -> bool {
    list().iter().any(|s| s.id == session)
}

/// `looop kill <id>` — terminate a session. Prints babysit's own result line
/// (this is a user-facing verb, so its output is wanted). Returns Ok on success.
pub fn kill(session: &str) -> anyhow::Result<()> {
    rt().block_on(::babysit::sub::kill(Some(session.to_string()), false))
}

/// `looop flag <id> [msg]` — raise a worker's attention flag.
pub fn flag(session: &str, message: Option<String>) -> anyhow::Result<()> {
    rt().block_on(::babysit::sub::flag(
        Some(session.to_string()),
        message,
        false,
    ))
}

/// `looop unflag <id>` — clear a worker's attention flag.
pub fn unflag(session: &str) -> anyhow::Result<()> {
    rt().block_on(::babysit::sub::unflag(Some(session.to_string()), false))
}

/// `looop attach <id>` — attach the terminal to a session; returns its exit code.
pub fn attach(session: &str) -> anyhow::Result<i32> {
    rt().block_on(::babysit::attach::attach(Some(session.to_string())))
}

/// Is a session currently alive?
pub fn is_alive(session: &str) -> bool {
    list().iter().any(|s| s.id == session && s.alive)
}

/// Any looop worker currently in flight?
pub fn any_looop_alive() -> bool {
    list_looop().iter().any(|s| s.alive)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sess(id: &str, note: Option<&str>) -> Session {
        Session {
            id: id.to_string(),
            note: note.map(str::to_string),
            ..Default::default()
        }
    }

    #[test]
    fn is_looop_matches_only_prefixed_ids() {
        assert!(sess("looop-triage", None).is_looop());
        assert!(!sess("other-job", None).is_looop());
        assert!(!sess("looop", None).is_looop()); // needs the trailing dash
    }

    #[test]
    fn flagged_iff_nonempty_note() {
        assert!(sess("looop-x", Some("help")).flagged());
        assert!(!sess("looop-x", Some("")).flagged());
        assert!(!sess("looop-x", None).flagged());
    }
}
