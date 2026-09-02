use std::path::Path;

use engine_cli::SimSource;

const ANIMATION_SCENE: &str = "tests/fixtures/scenes/animation_demo.toml";
const ASSETS_DIR: &str = "tests/fixtures/assets";

fn entity<'a>(json: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
    json["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["components"]["SceneName"] == name)
        .unwrap()
}

fn run(seed: u64, ticks: u64) -> serde_json::Value {
    engine_cli::run_and_dump_with_assets_dir(
        SimSource::Scene(ANIMATION_SCENE.into()),
        seed,
        ticks,
        Path::new(ASSETS_DIR),
    )
    .unwrap()
}

/// `crates/engine-assets/tests/fixtures/skinned.gltf` is a hand-built
/// two-joint rig: a root joint (identity rest pose) and a forearm joint
/// (rest translation `[0, 1, 0]`) with a rotation clip taking the forearm
/// from 0° to 90° about Z over 1 second. At tick 30 (0.5s at the scene's
/// default 1/60s timestep), the forearm should be at exactly 45°.
#[test]
fn joint_palette_matches_a_hand_computed_pose_at_a_known_tick() {
    let json = run(1, 30);
    let arm = entity(&json, "arm");
    let matrices = arm["components"]["JointPalette"]["matrices"]
        .as_array()
        .unwrap();
    assert_eq!(matrices.len(), 2, "expected one matrix per skeleton joint");

    // Root joint is never animated by the clip — its skinning matrix
    // should stay the identity (its inverse bind matrix is also identity).
    let root: Vec<f64> = matrices[0]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|col| col.as_array().unwrap().iter().map(|v| v.as_f64().unwrap()))
        .collect();
    let identity = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    for (got, want) in root.iter().zip(identity.iter()) {
        assert!(
            (got - want).abs() < 1e-4,
            "root joint drifted from identity: {root:?}"
        );
    }

    // Forearm joint: world = T(0,1,0) * R(45°about Z), skin = world *
    // inverse_bind (inverse_bind = T(0,-1,0)) — hand-derived translation
    // column [sin45, 1-cos45, 0] = [0.70711, 0.29289, 0].
    let forearm = matrices[1].as_array().unwrap();
    let translation: Vec<f64> = forearm[3]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect();
    let expected = [
        std::f64::consts::FRAC_1_SQRT_2,
        1.0 - std::f64::consts::FRAC_1_SQRT_2,
        0.0,
    ];
    for (got, want) in translation.iter().zip(expected.iter()) {
        assert!(
            (got - want).abs() < 1e-3,
            "forearm translation column {translation:?} doesn't match hand-computed {expected:?}"
        );
    }
}

#[test]
fn animation_scene_is_deterministic() {
    assert!(engine_cli::verify_scenario_determinism_with_assets_dir(
        SimSource::Scene(ANIMATION_SCENE.into()),
        7,
        30,
        Path::new(ASSETS_DIR),
    )
    .is_ok());
}

/// Clip sampling never touches the seeded RNG, a strictly stronger
/// determinism property than same-seed-twice: different seeds should
/// produce byte-identical `JointPalette` output too.
#[test]
fn different_seeds_produce_identical_joint_palettes() {
    let mut a = run(1, 30);
    let mut b = run(99, 30);
    a["seed"] = serde_json::Value::Null;
    b["seed"] = serde_json::Value::Null;
    assert_eq!(a, b);
}
