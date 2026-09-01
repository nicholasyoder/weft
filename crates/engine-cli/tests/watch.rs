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
