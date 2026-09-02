//! Proves `playground.toml`'s "rig" entity (a decorative, always-looping
//! skinned+animated prop, see debt-cleanup-plan.md Phase 8) actually
//! animates in the real sandbox scene+registry, not just in `engine-anim`'s
//! own isolated crate-level tests. `animation_step` silently no-ops without
//! an `AssetsDir` resource (see `engine-cli::SimSource::build`) — unlike
//! `pickup.rs`/`hud.rs`, this test has to insert one itself, since neither
//! `engine_scene::load` nor `sandbox::registry()` does.

use std::path::PathBuf;

use engine_anim::Animator;
use engine_core::{AssetsDir, JointPalette};

const SCENE: &str = "scenes/playground.toml";

#[test]
fn rig_joint_palette_changes_as_the_animation_plays() {
    let (components, systems) = sandbox::registry();
    let (mut sim, _dumpers) = engine_scene::load(SCENE.as_ref(), 1, &components, &systems).unwrap();
    sim.resources.insert(AssetsDir(PathBuf::from("assets")));

    let rig = sim
        .world
        .query::<&Animator>()
        .iter()
        .next()
        .map(|(e, _)| e)
        .expect("playground.toml should have one Animator-tagged entity");

    sim.step().unwrap();
    let first = sim
        .world
        .get::<&JointPalette>(rig)
        .expect("animation_step should have written a JointPalette by now")
        .matrices
        .clone();

    sim.run(30).unwrap();
    let later = sim
        .world
        .get::<&JointPalette>(rig)
        .unwrap()
        .matrices
        .clone();

    assert_ne!(
        first, later,
        "expected the rig's joint palette to change as its clip plays"
    );
}
