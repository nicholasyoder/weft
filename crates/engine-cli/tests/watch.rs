mod support;

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use support::WatchProcess;

const BASIC: &str = include_str!("fixtures/scenes/basic.toml");
const BASIC_MODIFIED: &str = include_str!("fixtures/scenes/basic_modified.toml");
const TIMEOUT: Duration = Duration::from_secs(10);

fn scratch_dir(name: &str) -> PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "engine-cli-watch-test-{}-{name}-{n}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const INCREMENT_X: &str = "function on_tick(components, tick, dt)\n  return { Position = { x = components.Position.x + 1.0, y = components.Position.y, z = components.Position.z } }\nend\n";

fn scripted_scene(script_path: &std::path::Path) -> String {
    format!(
        "[meta]\ndt = 0.016666667\n\n[[entity]]\nname = \"mover\"\n\n[entity.components.Position]\nx = 0.0\ny = 0.0\nz = 0.0\n\n[entity.components.Script]\npath = \"{}\"\nfunction = \"on_tick\"\n",
        script_path.display()
    )
}

#[test]
fn scene_edit_takes_effect_without_restarting() {
    let dir = scratch_dir("scene-reload");
    let scene_path = dir.join("scene.toml");
    std::fs::write(&scene_path, BASIC).unwrap();

    let mut proc = WatchProcess::spawn(&scene_path);
    let first = proc.next_event(TIMEOUT);
    assert_eq!(first["status"], "ok");
    assert_eq!(proc.next_event(TIMEOUT)["event"], "watching");

    std::fs::write(&scene_path, BASIC_MODIFIED).unwrap();
    let second = proc.next_event(TIMEOUT);
    assert_eq!(second["status"], "ok");
    assert_ne!(
        first["world"], second["world"],
        "editing the live scene file should change the next run's output"
    );
    assert!(
        proc.is_alive(),
        "the same process should still be running after picking up the edit"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn lua_edit_takes_effect_without_restarting() {
    let dir = scratch_dir("lua-reload");
    let script_path = dir.join("script.lua");
    std::fs::write(&script_path, INCREMENT_X).unwrap();
    let scene_path = dir.join("scene.toml");
    std::fs::write(&scene_path, scripted_scene(&script_path)).unwrap();

    let mut proc = WatchProcess::spawn(&scene_path);
    let first = proc.next_event(TIMEOUT);
    assert_eq!(first["status"], "ok");
    assert_eq!(proc.next_event(TIMEOUT)["event"], "watching");

    let increment_by_100 = INCREMENT_X.replace("+ 1.0", "+ 100.0");
    std::fs::write(&script_path, increment_by_100).unwrap();
    let second = proc.next_event(TIMEOUT);
    assert_eq!(second["status"], "ok");
    assert_ne!(
        first["world"], second["world"],
        "editing the referenced Lua script should change the next run's output"
    );

    std::fs::remove_dir_all(&dir).ok();
}

/// Regression test for known-issues.md's "`--watch`'s file debouncer is
/// torn down for the entire duration of every rerun": the old code dropped
/// the debouncer before starting a rerun and only created a fresh one
/// after it finished, so an edit landing during that window (which grows
/// with `--ticks`) was never observed. A large `--ticks` count on a plain,
/// script-free scene makes each rerun take real wall-clock time (a portable,
/// deterministic way to widen the window — sandboxed Lua scripts have no
/// sleep primitive to do this from inside a script instead), which this
/// test uses to fire a second edit while the first edit's rerun is still
/// in flight, then asserts the second edit is still eventually observed
/// with no further edits from the test.
#[test]
fn an_edit_landing_mid_rerun_is_not_lost() {
    const SLOW_TICKS: u64 = 300_000; // ~1s/rerun in a debug build, well under the 10s event timeout
    let dir = scratch_dir("mid-rerun-edit");
    let scene_path = dir.join("scene.toml");
    std::fs::write(&scene_path, BASIC).unwrap();

    let mut proc = WatchProcess::spawn_with_ticks(&scene_path, SLOW_TICKS);
    let first = proc.next_event(TIMEOUT);
    assert_eq!(first["status"], "ok");
    assert_eq!(proc.next_event(TIMEOUT)["event"], "watching");

    // Edit #1 triggers a ~1s rerun. Partway through it (well before it can
    // have finished), fire edit #2 — under the old code, the debouncer was
    // dropped for that entire rerun, so this edit would be silently missed
    // with no future trigger to ever pick it up.
    let second_velocity = "1.7"; // distinct from both BASIC's 0.5 and BASIC_MODIFIED's 0.9
    std::fs::write(&scene_path, BASIC_MODIFIED).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    std::fs::write(&scene_path, BASIC_MODIFIED.replace("0.9", second_velocity)).unwrap();

    // However many "ok" events the two edits produce (one coalesced or two
    // separate reload cycles — either is fine, and each reload cycle also
    // emits its own interleaved "watching" event), the last "ok" observed
    // must reflect edit #2, not stall on edit #1 forever. Bounded at 6
    // events (comfortably more than two full reload cycles need) rather
    // than looping forever, so a truly lost edit fails via the assert
    // below instead of hanging.
    let mut last_ok = None;
    let mut ok_count = 0;
    for _ in 0..6 {
        let event = proc.next_event(TIMEOUT);
        if event["status"] == "ok" {
            last_ok = Some(event);
            ok_count += 1;
            if ok_count == 2 {
                break;
            }
        }
    }
    let last_ok = last_ok.expect("expected at least one more successful reload");
    let mover_0_velocity_x = last_ok["world"]["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["components"]["SceneName"] == "mover-0")
        .expect("mover-0 entity present")["components"]["Velocity"]["x"]
        .as_f64()
        .unwrap();
    assert!(
        (mover_0_velocity_x - second_velocity.parse::<f64>().unwrap()).abs() < 1e-4,
        "the edit that landed mid-rerun must still be observed eventually: {last_ok}"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn broken_lua_edit_reports_a_structured_error_and_keeps_running() {
    let dir = scratch_dir("lua-error");
    let script_path = dir.join("script.lua");
    std::fs::write(&script_path, INCREMENT_X).unwrap();
    let scene_path = dir.join("scene.toml");
    std::fs::write(&scene_path, scripted_scene(&script_path)).unwrap();

    let mut proc = WatchProcess::spawn(&scene_path);
    let first = proc.next_event(TIMEOUT);
    assert_eq!(first["status"], "ok");
    assert_eq!(proc.next_event(TIMEOUT)["event"], "watching");

    std::fs::write(
        &script_path,
        "function on_tick(components, tick, dt\n  this is not valid lua\n",
    )
    .unwrap();
    let broken = proc.next_event(TIMEOUT);
    assert_eq!(broken["status"], "error");
    assert_eq!(broken["code"], "SCRIPT_LOAD_ERROR");
    assert!(
        proc.is_alive(),
        "a broken script edit must be reported, not crash the running process"
    );
    assert_eq!(proc.next_event(TIMEOUT)["event"], "watching");

    std::fs::write(&script_path, INCREMENT_X).unwrap();
    let recovered = proc.next_event(TIMEOUT);
    assert_eq!(
        recovered["status"], "ok",
        "fixing the script should let the same process recover"
    );

    std::fs::remove_dir_all(&dir).ok();
}
