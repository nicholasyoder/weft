//! `engine run --watch`: reruns a scene (and any Lua scripts it references)
//! from scratch whenever the scene file or a script file changes, without
//! restarting the process. See ADR-0006 for why this is one rebuild-and-
//! rerun mechanism rather than a cheaper in-place patch, and why it reruns
//! the existing `--ticks` budget rather than pacing to wall-clock time.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};

use crate::commands::OutputFormat;
use crate::diagnostics::CliError;
use crate::SimSource;

const DEBOUNCE: Duration = Duration::from_millis(200);

pub fn run(
    scene: &Path,
    seed: u64,
    ticks: u64,
    assets_dir: &Path,
    format: OutputFormat,
) -> ExitCode {
    if ticks == 0 {
        CliError::invalid_ticks(ticks).print(format.is_json());
        return ExitCode::FAILURE;
    }

    let mut watch_paths = match build_run_and_print(scene, seed, ticks, assets_dir, format, "run") {
        Ok(paths) => paths,
        Err(()) => return ExitCode::FAILURE,
    };

    // Created once and kept alive for the whole `watch` session — never
    // dropped/recreated per iteration. Dropping it around each rerun (the
    // previous design) meant an edit landing during a rerun was simply
    // never observed, and the window for that grows with `--ticks`. Kept
    // alive, the same edit just queues in `rx` and is picked up on the
    // very next `recv()` once the rerun finishes.
    let (tx, rx) = std::sync::mpsc::channel();
    let mut debouncer = match new_debouncer(DEBOUNCE, tx) {
        Ok(d) => d,
        Err(e) => {
            CliError::new("WATCH_INIT_ERROR", e.to_string()).print(format.is_json());
            return ExitCode::FAILURE;
        }
    };
    // Directories already registered with `debouncer` — a scene edit can
    // introduce a script in a directory not seen before, so this only ever
    // grows; a directory for a since-removed script path is left watched
    // too (harmless — `notify` per-directory watches are cheap and this
    // never needs fewer than it already has).
    let mut watched_dirs: HashSet<PathBuf> = HashSet::new();

    loop {
        for dir in watch_dirs(&watch_paths) {
            if watched_dirs.contains(&dir) {
                continue;
            }
            if let Err(e) = debouncer.watcher().watch(&dir, RecursiveMode::NonRecursive) {
                CliError::new(
                    "WATCH_INIT_ERROR",
                    format!("failed to watch '{}': {e}", dir.display()),
                )
                .print(format.is_json());
                return ExitCode::FAILURE;
            }
            watched_dirs.insert(dir);
        }

        let canonical: HashSet<PathBuf> =
            watch_paths.iter().filter_map(|p| canonicalize(p)).collect();

        // Signals that the watcher is armed and edits from here on will be
        // observed — without this, a caller (or a test) editing a file
        // immediately after the "run"/"reload" line races the `watch()`
        // calls above and can miss the very next edit.
        print_watching_event(format, &canonical);

        let changed = loop {
            match rx.recv() {
                Ok(Ok(events)) => {
                    let hit = events.iter().any(|e| {
                        e.kind == DebouncedEventKind::Any
                            && canonicalize(&e.path).is_some_and(|p| canonical.contains(&p))
                    });
                    if hit {
                        break true;
                    }
                }
                Ok(Err(_)) => continue,
                Err(_) => break false,
            }
        };
        if !changed {
            // The watch channel disconnected (debouncer thread gone) rather
            // than a real file change — nothing more to watch for.
            return ExitCode::SUCCESS;
        }

        match build_run_and_print(scene, seed, ticks, assets_dir, format, "reload") {
            Ok(paths) => watch_paths = paths,
            Err(()) => {
                // A reload error is reported (see build_run_and_print) but
                // must never kill the loop — keep watching the same paths
                // and wait for the next edit to fix it.
            }
        }
    }
}

/// Builds+runs `scene` once, prints a `{"event": event_name, ...}` line
/// (success or structured error), and on success returns the full set of
/// paths to watch (the scene file plus every distinct script it loaded).
fn build_run_and_print(
    scene: &Path,
    seed: u64,
    ticks: u64,
    assets_dir: &Path,
    format: OutputFormat,
    event_name: &str,
) -> Result<Vec<PathBuf>, ()> {
    match crate::run_and_dump_with_script_paths(
        SimSource::Scene(scene.to_path_buf()),
        seed,
        ticks,
        assets_dir,
    ) {
        Ok((world, mut paths)) => {
            paths.push(scene.to_path_buf());
            if format.is_json() {
                println!(
                    "{}",
                    serde_json::json!({
                        "event": event_name,
                        "status": "ok",
                        "scene": scene.display().to_string(),
                        "seed": seed,
                        "ticks": ticks,
                        "world": world,
                    })
                );
            } else {
                println!(
                    "[{event_name}] ran '{}' for {ticks} ticks at seed {seed}",
                    scene.display()
                );
            }
            Ok(paths)
        }
        Err(e) => {
            if format.is_json() {
                println!(
                    "{}",
                    serde_json::json!({
                        "event": event_name,
                        "status": "error",
                        "code": e.code,
                        "message": e.message,
                    })
                );
            } else {
                println!("[{event_name}] error[{}]: {}", e.code, e.message);
            }
            Err(())
        }
    }
}

fn print_watching_event(format: OutputFormat, paths: &HashSet<PathBuf>) {
    if format.is_json() {
        println!(
            "{}",
            serde_json::json!({
                "event": "watching",
                "paths": paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
            })
        );
    } else {
        println!("[watching] {} path(s)", paths.len());
    }
}

/// The deduplicated set of directories to watch (non-recursively) so that
/// editor save patterns which replace the inode (temp-file + rename) still
/// trigger an event — watching the file itself would miss those.
fn watch_dirs(paths: &[PathBuf]) -> HashSet<PathBuf> {
    paths
        .iter()
        .filter_map(|p| p.parent().map(Path::to_path_buf))
        .map(|dir| {
            if dir.as_os_str().is_empty() {
                PathBuf::from(".")
            } else {
                dir
            }
        })
        .collect()
}

fn canonicalize(path: &Path) -> Option<PathBuf> {
    std::fs::canonicalize(path).ok()
}
