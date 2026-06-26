//! Runner-wiring config — the only thing that needs "installing".
//!
//! Written to $LOOOP_CONFIG by `looop init` (the interactive setup wizard). It
//! wires up ONE runner with three commands at the top level (no profiles): `tick`
//! = how to run one disposable AI move (stdin = the tick prompt). `interactive` =
//! how to launch a worker agent; {{prompt_file}} is substituted with the worker's
//! prompt file. `resume` = how to re-attach a worker session.
//!
//! NOT INITIALIZED = no config file. `looop up` REFUSES to start the pulse in
//! that state and tells the operator to run `looop init`, which lets you EDIT the
//! three command strings (prefilled with the current values, or the claude
//! default on first run) and writes the wiring. The inline `DEFAULT_CONFIG`
//! (claude) is the only wiring this code knows about; it is both the first-run
//! default and the safety net for `Config::load` (e.g. a `_` verb that runs
//! without a file).
//!
//! looop is deliberately a GLUE layer: it does NOT bake in per-runner command
//! knowledge (codex/opencode/pi flags, model ids, …). The only runner literal in
//! code is the single claude default below; ready-to-paste wirings for other
//! runners live in the README, and `looop init` just lets you edit the strings.
//!
//! TICK OUTPUT (H3): `runner::run_streamed` renders every tick IN-PROCESS off the
//! runner's NDJSON stdout. Both runners therefore need their structured stream
//! enabled in the tick command (pi: `--mode json`, claude: `--output-format
//! stream-json --verbose`), but NEITHER pipes through an external formatter —
//! there is no `| _ fmt` seam anymore.
//!
//! BACK-COMPAT: a stored config written before this change may still end
//! its tick command with `| "$LOOOP_BIN" _ fmt`. `runner_cmd` strips that trailing
//! seam on load (see `strip_fmt_seam`), so old configs keep working unchanged.
//!
//! MODEL ALLOCATION: the tick is one tiny decision (pick the single next move),
//! so the default claude wiring runs it on the fast model (`--model sonnet`);
//! workers do the heavy multi-step execution on the stronger model (`--model
//! opus`). Spend stays bounded because the world-hash gate skips the AI entirely
//! when nothing changed, and the tick emits only one tiny decision. Tune by
//! editing the commands (`looop init` or the file directly).

use crate::paths::Paths;
use anyhow::{Context, Result};
use std::fs;

/// The inline default config. Originally a copy of the bash `default_config`,
/// it now diverges deliberately: the tick commands no longer carry the
/// `| "$LOOOP_BIN" _ fmt` seam, since output formatting runs in-process
/// (see `runner::run_streamed`).
pub const DEFAULT_CONFIG: &str = r#"{
  "tick": "claude -p --output-format stream-json --verbose --dangerously-skip-permissions --model sonnet",
  "interactive": "claude --dangerously-skip-permissions --model opus \"$(cat {{prompt_file}})\"",
  "resume": "claude --resume"
}
"#;

/// The three keys a wiring is made of, in the order `looop init` prompts for
/// them. Editing-only metadata: `looop` itself reads them via [`Config`].
pub const KEYS: [&str; 3] = ["tick", "interactive", "resume"];

/// Assemble the wiring JSON from the three command strings the user supplied to
/// `looop init`. Pure serialization — NO per-runner knowledge lives here; the
/// commands are whatever the operator typed (seeded from the claude default).
pub fn wiring_json(tick: &str, interactive: &str, resume: &str) -> String {
    let v = serde_json::json!({
        "tick": tick,
        "interactive": interactive,
        "resume": resume,
    });
    serde_json::to_string_pretty(&v).expect("config json") + "\n"
}

/// The parsed config — kept as a generic JSON value so the runner table stays
/// open-ended (mirrors the bash `jq` lookups rather than a rigid schema).
pub struct Config {
    pub root: serde_json::Value,
}

impl Config {
    /// Load $LOOOP_CONFIG, falling back to the inline default when absent
    /// (matches bash: `[ -f "$CONFIG" ] && cat || default_config`).
    pub fn load(paths: &Paths) -> Result<Self> {
        let text = if paths.config.is_file() {
            fs::read_to_string(&paths.config)
                .with_context(|| format!("reading config {}", paths.config.display()))?
        } else {
            DEFAULT_CONFIG.to_string()
        };
        let root: serde_json::Value =
            serde_json::from_str(&text).context("parsing looop config JSON")?;
        Ok(Config { root })
    }

    /// A short label for the configured runner — the first token of the `tick`
    /// command (e.g. `pi`, `claude`). For log lines only.
    pub fn runner_label(&self) -> String {
        self.runner_cmd("tick")
            .or_else(|| self.runner_cmd("interactive"))
            .and_then(|c| c.split_whitespace().next().map(str::to_owned))
            .unwrap_or_else(|| "runner".into())
    }

    /// `.<key>` — e.g. the `tick` / `interactive` / `resume` command.
    ///
    /// Any trailing `| "$LOOOP_BIN" _ fmt` seam from a pre-in-process-metering
    /// config is stripped here (`strip_fmt_seam`), so stored configs written
    /// before the formatter moved in-process keep working without re-seeding.
    pub fn runner_cmd(&self, key: &str) -> Option<String> {
        self.root.get(key)?.as_str().map(strip_fmt_seam)
    }
}

/// Strip a trailing `| <bin> _ fmt` (or `_fmt`) seam from a runner command.
///
/// Tick output formatting moved in-process (`runner::run_streamed`), so
/// the old external pipe is dead. Older configs still carry it; rather
/// than force a re-seed (which would clobber user edits) we drop the seam on load.
/// Only the LAST pipe segment is inspected, and only when it is recognisably the
/// fmt seam (mentions the looop binary and ends in `_ fmt`/`_fmt`) — any other
/// user pipeline is left untouched.
fn strip_fmt_seam(cmd: &str) -> String {
    if let Some(idx) = cmd.rfind('|') {
        let tail: String = cmd[idx + 1..]
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let is_fmt = (tail.ends_with("_ fmt") || tail.ends_with("_fmt"))
            && (tail.contains("LOOOP_BIN") || tail.contains("looop"));
        if is_fmt {
            return cmd[..idx].trim_end().to_string();
        }
    }
    cmd.to_string()
}

/// True once the operator has run `looop init` (the config file exists). `looop
/// up` gates on this and refuses to start the pulse when false, directing the
/// user to `looop init`.
pub fn is_initialized(paths: &Paths) -> bool {
    paths.config.is_file()
}

/// Write the runner wiring to $LOOOP_CONFIG (creating its parent dir). Used by
/// `looop init`; always overwrites any existing file.
pub fn write(paths: &Paths, contents: &str) -> Result<()> {
    if let Some(dir) = paths.config.parent() {
        fs::create_dir_all(dir)
            .with_context(|| format!("creating config dir {}", dir.display()))?;
    }
    fs::write(&paths.config, contents)
        .with_context(|| format!("writing config {}", paths.config.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::strip_fmt_seam;

    #[test]
    fn strips_trailing_fmt_seam() {
        assert_eq!(
            strip_fmt_seam("pi -p --mode json | \"$LOOOP_BIN\" _ fmt"),
            "pi -p --mode json"
        );
        // Joined verb form (`_fmt`) and bare `looop` binary token both match.
        assert_eq!(strip_fmt_seam("claude -p | looop _fmt"), "claude -p");
    }

    #[test]
    fn leaves_unrelated_pipelines_untouched() {
        let cmd = "pi -p --mode json | jq .";
        assert_eq!(strip_fmt_seam(cmd), cmd);
        let plain = "pi -p --mode json";
        assert_eq!(strip_fmt_seam(plain), plain);
        // A trailing pipe that is not the fmt seam stays put.
        let other = "claude -p | tee out.log";
        assert_eq!(strip_fmt_seam(other), other);
    }

    #[test]
    fn default_config_has_no_fmt_seam() {
        assert!(!super::DEFAULT_CONFIG.contains("_ fmt"));
        assert!(!super::DEFAULT_CONFIG.contains("_fmt"));
    }

    #[test]
    fn default_config_is_valid_claude_wiring() {
        let cfg = super::Config {
            root: serde_json::from_str(super::DEFAULT_CONFIG).expect("default config parses"),
        };
        assert_eq!(cfg.runner_label(), "claude");
        let inter = cfg.runner_cmd("interactive").unwrap();
        // The worker prompt placeholder survives JSON round-trip un-escaped.
        assert!(inter.contains("{{prompt_file}}"));
        assert!(inter.contains("$(cat"));
    }

    #[test]
    fn wiring_json_round_trips_the_three_commands() {
        let json = super::wiring_json("T cmd", "I {{prompt_file}}", "R cmd");
        let cfg = super::Config {
            root: serde_json::from_str(&json).expect("wiring json parses"),
        };
        assert_eq!(cfg.runner_cmd("tick").unwrap(), "T cmd");
        assert_eq!(cfg.runner_cmd("interactive").unwrap(), "I {{prompt_file}}");
        assert_eq!(cfg.runner_cmd("resume").unwrap(), "R cmd");
        // Keys match the documented edit order.
        assert_eq!(super::KEYS, ["tick", "interactive", "resume"]);
    }
}
