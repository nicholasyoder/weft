//! Subprocess smoke test for `sandbox --max-ticks N`, mirroring
//! `engine-cli/tests/watch.rs`'s "spawn the real binary" posture. Ignored
//! by default: unlike every other test in this workspace, this one needs a
//! real windowing backend (X11/Wayland/etc.) to create a window at all —
//! `Backends::VULKAN` + Mesa's lavapipe alone (this sandbox's environment)
//! isn't sufficient the way it is for `engine render`'s offscreen path (see
//! ADR-0010). Run explicitly once a windowing backend is available, e.g.:
//!
//!   xvfb-run cargo test -p sandbox -- --ignored

use assert_cmd::Command;

#[test]
#[ignore]
fn exits_cleanly_after_max_ticks() {
    let mut cmd = Command::cargo_bin("sandbox").unwrap();
    cmd.args(["scenes/playground.toml", "--max-ticks", "30"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .assert()
        .success();
}
