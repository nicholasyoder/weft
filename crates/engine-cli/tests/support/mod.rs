use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

/// A running `engine run --watch` subprocess with its stdout forwarded line
/// by line over a channel, for tests that need to observe a long-running
/// process reacting to file edits rather than a one-shot `assert_cmd` run.
pub struct WatchProcess {
    child: Child,
    events: Receiver<String>,
}

impl WatchProcess {
    pub fn spawn(scene: &Path) -> Self {
        Self::spawn_with_ticks(scene, 60)
    }

    /// Like `spawn`, but with an explicit `--ticks` instead of the
    /// (clap-level) default of 60 — lets a test make each run/rerun take
    /// real wall-clock time (a plain scene's per-tick cost is on the order
    /// of microseconds, so a large enough tick count is a deterministic,
    /// portable way to widen a rerun's duration without relying on a
    /// sleep primitive, which sandboxed Lua scripts don't have).
    pub fn spawn_with_ticks(scene: &Path, ticks: u64) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_engine"))
            .args([
                "run",
                &scene.display().to_string(),
                "--watch",
                "--ticks",
                &ticks.to_string(),
                "--format",
                "json",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to spawn `engine run --watch`");

        let stdout = child.stdout.take().expect("child stdout was not piped");
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                match line {
                    Ok(line) => {
                        if tx.send(line).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        Self { child, events: rx }
    }

    /// Blocks for the next stdout line and parses it as JSON, panicking if
    /// none arrives within `timeout` or it isn't valid JSON.
    pub fn next_event(&mut self, timeout: Duration) -> serde_json::Value {
        let line = self
            .events
            .recv_timeout(timeout)
            .expect("timed out waiting for a watch event");
        serde_json::from_str(&line)
            .unwrap_or_else(|e| panic!("event line was not valid JSON ({e}): {line}"))
    }

    pub fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
}

impl Drop for WatchProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
