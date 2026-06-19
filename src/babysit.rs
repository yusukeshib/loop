//! Wrapper over the `babysit` worker fleet.
//!
//! looop is a SINGLE self-contained binary — the `babysit` executable is not
//! required at runtime. Every data + control path runs against the babysit
//! LIBRARY in-process through an explicit [`Babysit`](::babysit::Babysit)
//! context: looop derives the state root from its own `LOOOP_DATA_DIR`
//! (`paths.sessions()`), so the fleet is self-contained per profile — no
//! `$BABYSIT_DIR`, no shared `~/.babysit`, and no `looop-` id prefix needed to
//! disambiguate a shared root.
//!
//! Detached spawn re-execs `current_exe()` (= looop) as the headless supervisor;
//! babysit hands the worker its state root via `--root` and the id via
//! `--detached-id`, which looop routes back into the library
//! (`run_detached_worker`) — so the worker is owned by a looop process with no
//! `babysit` binary and no environment-derived configuration.
//!
//! The leading `::babysit` disambiguates the extern crate from this module.

use crate::paths::Paths;

/// The session id the pulse (親玉) runs under when started as a service
/// (`looop up`). It is reserved: a worker can never take this id (see
/// `session::cmd_start_session`), so the single control-plane session can't
/// collide with a goal-named worker.
pub const PULSE_SESSION: &str = "pulse";

/// A process-wide multi-thread tokio runtime to drive babysit's async API.
/// looop is otherwise synchronous; async is confined to this boundary.
fn rt() -> &'static tokio::runtime::Runtime {
    use std::sync::OnceLock;
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        // Multi-thread + enable_all to match babysit's own `#[tokio::main]`:
        // the detached worker (serve_worker) owns a PTY read loop + a control
        // socket accept loop concurrently, and `attach` drives a socket + PTY.
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("looop: failed to build tokio runtime")
    })
}

/// One session in this profile's fleet — a thin projection of babysit's
/// `SessionInfo` onto just what looop reasons about.
#[derive(Debug, Default)]
pub struct Session {
    pub id: String,
    pub state: String,
    pub alive: bool,
    pub exit_code: Option<i64>,
    pub note: Option<String>,
}

impl Session {
    /// The pulse session is the 親玉, not a worker.
    pub fn is_pulse(&self) -> bool {
        self.id == PULSE_SESSION
    }
    /// True when the session has raised a flag (a non-empty note).
    pub fn flagged(&self) -> bool {
        self.note.as_deref().map(|n| !n.is_empty()).unwrap_or(false)
    }
}

fn project(info: ::babysit::SessionInfo) -> Session {
    Session {
        id: info.id,
        state: info.state,
        alive: info.alive,
        exit_code: info.exit_code.map(|c| c as i64),
        note: info.note,
    }
}

/// List every session in this profile's fleet. Any failure yields an empty
/// list: the pulse degrades gracefully, never wedges.
pub fn list(paths: &Paths) -> Vec<Session> {
    match rt().block_on(paths.sessions().list_sessions()) {
        Ok(sessions) => sessions.into_iter().map(project).collect(),
        Err(_) => Vec::new(),
    }
}

/// Worker sessions only — the pulse (親玉) is excluded. Everything that reasons
/// about "the fleet the pulse manages" (cadence, world hash, tick prompt,
/// status, flag-surfacing) uses this so the pulse never counts itself.
pub fn list_workers(paths: &Paths) -> Vec<Session> {
    list(paths).into_iter().filter(|s| !s.is_pulse()).collect()
}

/// Clear exited/dead-owner corpses, IN-PROCESS and SILENTLY. babysit's own
/// `prune` prints (it is a CLI handler), which would pollute looop's pretty
/// output, so we drive the same decision over the library's lower-level
/// session/context API. The fleet root is looop-exclusive, so every corpse here
/// is ours. Best-effort: any error is ignored.
pub fn prune(paths: &Paths) {
    use ::babysit::session::{self, State};
    let bs = paths.sessions();
    rt().block_on(async {
        let ids = match session::list_ids(&bs).await {
            Ok(ids) => ids,
            Err(_) => return,
        };
        for id in ids {
            let Ok(meta) = session::read_meta(&bs, &id).await else {
                continue; // unparseable meta — leave it alone, never nuke blind
            };
            let status = session::read_status(&bs, &id).await.ok();
            let alive = session::is_pid_alive(meta.babysit_pid);
            let dead = match status.as_ref().map(|s| s.state) {
                Some(State::Exited | State::Killed) => true,
                Some(State::Starting | State::Running) if !alive => true,
                None if !alive => true,
                _ => false,
            };
            if dead {
                let _ = tokio::fs::remove_dir_all(bs.session_dir(&id)).await;
            }
        }
    });
}

/// Does a session with this id exist in the fleet?
pub fn status_exists(paths: &Paths, session: &str) -> bool {
    list(paths).iter().any(|s| s.id == session)
}

/// `looop kill <id>` — terminate a session.
pub fn kill(paths: &Paths, session: &str) -> anyhow::Result<()> {
    rt().block_on(paths.sessions().kill(Some(session.to_string()), false))
}

/// `looop flag <id> [msg]` — raise a session's attention flag.
pub fn flag(paths: &Paths, session: &str, message: Option<String>) -> anyhow::Result<()> {
    rt().block_on(
        paths
            .sessions()
            .flag(Some(session.to_string()), message, false),
    )
}

/// `looop unflag <id>` — clear a session's attention flag.
pub fn unflag(paths: &Paths, session: &str) -> anyhow::Result<()> {
    rt().block_on(paths.sessions().unflag(Some(session.to_string()), false))
}

/// `looop attach <id>` — attach the terminal to a session; returns its exit code.
pub fn attach(paths: &Paths, session: &str) -> anyhow::Result<i32> {
    let bs = paths.sessions();
    rt().block_on(::babysit::attach::attach(&bs, Some(session.to_string())))
}

/// `looop detach <id>` — force-detach any other terminal attached to a session.
pub fn detach(paths: &Paths, session: &str, json: bool) -> anyhow::Result<()> {
    let bs = paths.sessions();
    rt().block_on(::babysit::attach::detach(
        &bs,
        Some(session.to_string()),
        json,
    ))
}

/// `looop watch <id>` — follow a session's output read-only (tail -f): the full
/// log so far, then live output until the session exits. The non-interactive
/// twin of `attach`.
pub fn watch(paths: &Paths, session: &str) -> anyhow::Result<()> {
    rt().block_on(paths.sessions().log(
        Some(session.to_string()),
        None,  // tail: whole log, then follow
        None,  // grep
        false, // raw
        None,  // since: from the start
        true,  // follow
        false, // json
    ))
}

/// `looop log <id>` — show / tail / grep / follow a session's recorded output.
#[allow(clippy::too_many_arguments)]
pub fn log(
    paths: &Paths,
    session: &str,
    tail: Option<usize>,
    grep: Option<String>,
    raw: bool,
    since: Option<u64>,
    follow: bool,
    json: bool,
) -> anyhow::Result<()> {
    rt().block_on(paths.sessions().log(
        Some(session.to_string()),
        tail,
        grep,
        raw,
        since,
        follow,
        json,
    ))
}

/// `looop shot <id>` — render the session's current visible screen.
pub fn screenshot(
    paths: &Paths,
    session: &str,
    format: ::babysit::cli::ShotFormat,
    trim: bool,
) -> anyhow::Result<()> {
    rt().block_on(
        paths
            .sessions()
            .screenshot(Some(session.to_string()), format, trim),
    )
}

/// `looop send <id> <text>` — type text into a session's stdin.
pub fn send(
    paths: &Paths,
    session: &str,
    text: String,
    newline: bool,
    json: bool,
) -> anyhow::Result<()> {
    rt().block_on(
        paths
            .sessions()
            .send(Some(session.to_string()), text, newline, json),
    )
}

/// `looop key <id> <KEY...>` — send named keys (Enter, Up, C-c, …) to a session.
pub fn key(paths: &Paths, session: &str, keys: Vec<String>, json: bool) -> anyhow::Result<()> {
    rt().block_on(paths.sessions().key(Some(session.to_string()), keys, json))
}

/// `looop expect <id> <REGEX>` — block until a regex appears; exit 124 on timeout.
#[allow(clippy::too_many_arguments)]
pub fn expect(
    paths: &Paths,
    session: &str,
    pattern: String,
    timeout: String,
    since: Option<u64>,
    from_now: bool,
    raw: bool,
    screen: bool,
    json: bool,
) -> anyhow::Result<i32> {
    rt().block_on(paths.sessions().expect(
        Some(session.to_string()),
        pattern,
        timeout,
        since,
        from_now,
        raw,
        screen,
        json,
    ))
}

/// `looop wait <id>` — block until the session exits; returns its exit code.
pub fn wait(paths: &Paths, session: &str, timeout: Option<String>) -> anyhow::Result<i32> {
    rt().block_on(paths.sessions().wait(Some(session.to_string()), timeout))
}

/// `looop wait-idle <id>` — block until output is quiet for `settle`.
pub fn wait_idle(
    paths: &Paths,
    session: &str,
    settle: String,
    timeout: String,
) -> anyhow::Result<i32> {
    rt().block_on(
        paths
            .sessions()
            .wait_idle(Some(session.to_string()), settle, timeout),
    )
}

/// `looop resize <id> <COLSxROWS>` — resize a session's terminal.
pub fn resize(paths: &Paths, session: &str, size: String, json: bool) -> anyhow::Result<()> {
    rt().block_on(
        paths
            .sessions()
            .resize(Some(session.to_string()), size, json),
    )
}

/// `looop restart <id>` — restart the wrapped command in a session.
pub fn restart(paths: &Paths, session: &str, json: bool) -> anyhow::Result<()> {
    rt().block_on(paths.sessions().restart(Some(session.to_string()), json))
}

/// `looop ls [--json] [--watch] [--interval <dur>]` — render the fleet table
/// IN-PROCESS via babysit's own list renderer.
pub fn ls(paths: &Paths, json: bool, watch: bool, interval: String) -> anyhow::Result<()> {
    rt().block_on(paths.sessions().list(json, watch, interval))
}

/// Spawn a detached worker IN-PROCESS. babysit's parent path re-execs
/// `current_exe()` (= looop) as the headless supervisor, handing it the state
/// root via `--root` and the id via `--detached-id`; looop routes that back into
/// `serve_worker` via `run_detached_worker`. babysit prints a start banner on
/// the parent path; we suppress it so looop owns its own "started …" output.
pub fn spawn_detached(paths: &Paths, cmd: Vec<String>, session: &str) -> anyhow::Result<()> {
    let bs = paths.sessions();
    suppress_stdout(|| {
        rt().block_on(bs.run(
            cmd,
            Some(session.to_string()),
            true,  // detach: spawn the worker and return immediately
            None,  // detached_id: we are the parent, not the worker
            false, // no_tty
            None,  // timeout
            None,  // idle_timeout
            None,  // size
            true,  // json (one suppressed line; we print our own message)
        ))
    })
    .map(|_code| ())
}

/// The worker side of detached spawn: looop was re-exec'd by babysit's detacher
/// as `looop run --detached-id <id> --root <dir> [--no-tty] [--timeout <ms>]
/// [--idle-timeout <ms>] [--size <CxR>] -- <cmd…>`. Parse that argv and hand off
/// to the library's headless supervisor, which blocks until the wrapped command
/// exits. The state root comes from `--root`, so the worker reconstructs THIS
/// fleet's context without reading any environment.
pub fn run_detached_worker(args: &[String]) -> anyhow::Result<i32> {
    use anyhow::Context;
    let mut id = None;
    let mut root = None;
    let mut no_tty = false;
    let mut timeout = None;
    let mut idle_timeout = None;
    let mut size = None;
    let mut cmd: Vec<String> = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--detached-id" => id = it.next().cloned(),
            "--root" => root = it.next().cloned(),
            "--no-tty" => no_tty = true,
            "--timeout" => timeout = it.next().cloned(),
            "--idle-timeout" => idle_timeout = it.next().cloned(),
            "--size" => size = it.next().cloned(),
            "--" => {
                cmd = it.by_ref().cloned().collect();
                break;
            }
            _ => {} // ignore unknown flags (forward-compat with babysit)
        }
    }
    let id = id.context("looop run --detached-id: missing worker id")?;
    let root = root.context("looop run --detached-id: missing --root")?;
    let bs = ::babysit::Babysit::new(root);
    rt().block_on(bs.run(
        cmd,
        None,
        false,
        Some(id),
        no_tty,
        timeout,
        idle_timeout,
        size,
        false,
    ))
}

/// Run `f` with this process's stdout (fd 1) redirected to /dev/null, then
/// restore it. Used to swallow babysit's parent-path banner while keeping
/// looop's own output. Unix-only; a no-op redirect failure just runs `f`.
#[cfg(unix)]
fn suppress_stdout<T>(f: impl FnOnce() -> T) -> T {
    use std::io::Write;
    use std::os::unix::io::AsRawFd;
    unsafe extern "C" {
        fn dup(fd: i32) -> i32;
        fn dup2(a: i32, b: i32) -> i32;
        fn close(fd: i32) -> i32;
    }
    let Ok(devnull) = std::fs::OpenOptions::new().write(true).open("/dev/null") else {
        return f();
    };
    let _ = std::io::stdout().flush();
    unsafe {
        let saved = dup(1);
        if saved < 0 {
            return f();
        }
        dup2(devnull.as_raw_fd(), 1);
        let out = f();
        let _ = std::io::stdout().flush();
        dup2(saved, 1);
        close(saved);
        out
    }
}

#[cfg(not(unix))]
fn suppress_stdout<T>(f: impl FnOnce() -> T) -> T {
    f()
}

/// Is a session currently alive?
pub fn is_alive(paths: &Paths, session: &str) -> bool {
    list(paths).iter().any(|s| s.id == session && s.alive)
}

/// Any looop worker currently in flight?
pub fn any_worker_alive(paths: &Paths) -> bool {
    list_workers(paths).iter().any(|s| s.alive)
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
    fn pulse_is_recognized() {
        assert!(sess(PULSE_SESSION, None).is_pulse());
        assert!(!sess("triage", None).is_pulse());
    }

    #[test]
    fn flagged_iff_nonempty_note() {
        assert!(sess("x", Some("help")).flagged());
        assert!(!sess("x", Some("")).flagged());
        assert!(!sess("x", None).flagged());
    }
}
