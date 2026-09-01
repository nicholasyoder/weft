//! CLI-level proof for ADR-0012's `engine.random`/`engine.despawn`/
//! `engine.query` Lua bindings, mirroring `despawn.rs`'s pattern: exercise
//! the real scene/CLI surface, not just `engine-script`'s in-process tests.

use engine_cli::SimSource;

const SCENE: &str = "tests/fixtures/scenes/scripted_gameplay.toml";

fn entity_names(json: &serde_json::Value) -> Vec<String> {
    json["entities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["components"]["SceneName"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn collector_despawns_only_the_pickup_within_range() {
    let json = engine_cli::run_and_dump(SimSource::Scene(SCENE.into()), 1, 1).unwrap();
    let names = entity_names(&json);
    assert!(names.contains(&"collector".to_string()));
    assert!(
        !names.contains(&"pickup-near".to_string()),
        "pickup-near should be despawned via engine.query + engine.despawn(id)"
    );
    assert!(
        names.contains(&"pickup-far".to_string()),
        "pickup-far is out of range and should survive"
    );
}

#[test]
fn fuse_despawns_itself_after_a_randomized_countdown() {
    // engine.random_int(2, 4) rolls on tick 1, so the fuse is gone well
    // before tick 10 regardless of the roll.
    let json = engine_cli::run_and_dump(SimSource::Scene(SCENE.into()), 1, 10).unwrap();
    assert!(!entity_names(&json).contains(&"fuse".to_string()));
}

#[test]
fn scripted_gameplay_scene_is_deterministic() {
    assert!(engine_cli::verify_scenario_determinism(SimSource::Scene(SCENE.into()), 7, 10).is_ok());
}

#[test]
fn different_seeds_can_roll_different_fuse_countdowns() {
    // Not a strict guarantee for any single pair of seeds, but with a 3-way
    // roll (2, 3, or 4) across several seeds at least one pair should
    // differ; this is a real assertion on engine.random_int actually
    // drawing from the seeded stream, not a fixed constant.
    let despawn_tick = |seed: u64| -> u64 {
        for ticks in 1..=6 {
            let json =
                engine_cli::run_and_dump(SimSource::Scene(SCENE.into()), seed, ticks).unwrap();
            if !entity_names(&json).contains(&"fuse".to_string()) {
                return ticks;
            }
        }
        panic!("fuse never despawned within 6 ticks for seed {seed}");
    };
    let ticks: std::collections::HashSet<u64> = (1..=5).map(despawn_tick).collect();
    assert!(
        ticks.len() > 1,
        "expected at least two different despawn ticks across 5 seeds, got {ticks:?}"
    );
}

#[test]
fn engine_key_held_works_through_the_batch_cli_path_with_no_live_input() {
    // ADR-0013: batch commands (run/test/inspect/replay) have no live input
    // source, so `engine.key_held` should resolve every key as not-held
    // without erroring — this is a "the binding works end-to-end" check,
    // not a live-input check (engine-script's own tests cover that).
    const SCENE: &str = "tests/fixtures/scenes/reads_key_held.toml";
    let json = engine_cli::run_and_dump(SimSource::Scene(SCENE.into()), 1, 1).unwrap();
    let x = json["entities"][0]["components"]["Position"]["x"]
        .as_f64()
        .unwrap();
    assert_eq!(
        x, 0.0,
        "no live input source means every key reads not-held"
    );
}
