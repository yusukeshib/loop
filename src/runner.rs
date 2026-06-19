//! ACT — run the configured tick runner once, teeing its output to the per-beat
//! archive (runs/<id>/output.log) and tick.log so a beat is replayable. The
//! caller chooses whether the runner's free-form output is ALSO echoed live
//! (`live`): the pulse keeps its stream a clean structured-event log and only
//! archives the chatter, while a manual `looop run <goal>` streams it so you can
//! watch that one move. A port of the bash
//! `( cd DATA && eval "$tick_cmd" < prompt ) 2>&1 | ts_prefix | tee …` pipeline.

use crate::paths::Paths;
use crate::util;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Run `tick_cmd` (a shell pipeline) under `bash -lc`, with cwd at the data dir
/// and stdin from `prompt_file`. stdout+stderr are merged, each line is stamped
/// and written to every `tee` file. When `live` is set (and not JSON mode) the
/// stamped line is ALSO echoed to stdout. Returns whether the runner exited
/// successfully.
pub fn run_streamed(
    paths: &Paths,
    tick_cmd: &str,
    prompt_file: &Path,
    cost_env: &[(&str, &str)],
    tee: &[PathBuf],
    live: bool,
) -> bool {
    let stdin = match File::open(prompt_file) {
        Ok(f) => f,
        Err(_) => return false,
    };

    // `{ …; } 2>&1` merges the whole pipeline's stderr into stdout in order, so
    // a single pipe carries everything (Rust can't easily interleave two pipes).
    let script = format!("{{ {tick_cmd} ; }} 2>&1");

    let mut cmd = Command::new("bash");
    cmd.arg("-lc")
        .arg(&script)
        .current_dir(&paths.data_dir)
        .stdin(Stdio::from(stdin))
        .stdout(Stdio::piped());
    for (k, v) in cost_env {
        cmd.env(k, v);
    }

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(_) => return false,
    };
    let Some(out) = child.stdout.take() else {
        return false;
    };

    let mut sinks: Vec<File> = tee.iter().filter_map(|p| File::create(p).ok()).collect();
    let mut stdout = std::io::stdout();
    // Echo the runner's chatter to stdout only when the caller asked for a live
    // view AND we're not emitting a machine NDJSON stream (which it would
    // corrupt). The pulse passes live=false: its stream stays structured events.
    let echo = live && !util::is_json();

    for line in BufReader::new(out).lines() {
        let Ok(line) = line else { break };
        let prefix = format!("{}[{}]{} ", util::dim(), util::hms(), util::rst());
        // Archive the runner's output verbatim (replay fidelity)…
        for f in &mut sinks {
            let _ = writeln!(f, "{prefix}{line}");
        }
        // …and, for a manual run, echo it flat: drop the runner's own leading
        // indentation so every line sits under the timestamp, not stair-stepped.
        if echo {
            let _ = writeln!(stdout, "{prefix}{}", line.trim_start());
            let _ = stdout.flush();
        }
    }

    child.wait().map(|s| s.success()).unwrap_or(false)
}
