use assert_cmd::Command;
use engine_cli::{DeterminismResult, SimSource};

const BASIC_SCENE: &str = "tests/fixtures/scenes/basic.toml";
const BASIC_SCENE_MODIFIED: &str = "tests/fixtures/scenes/basic_modified.toml";

#[test]
fn scene_loads_and_runs_with_scene_name_and_component_fields() {
    let json = engine_cli::run_and_dump(SimSource::Scene(BASIC_SCENE.into()), 1, 60).unwrap();
    let entities = json["entities"].as_array().unwrap();
    assert_eq!(entities.len(), 3);

    let names: Vec<&str> = entities
        .iter()
        .map(|e| e["components"]["SceneName"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"mover-0"));
    assert!(names.contains(&"mover-1"));
    assert!(names.contains(&"static-marker"));
}

#[test]
fn scene_is_deterministic() {
    assert!(
        engine_cli::verify_scenario_determinism(SimSource::Scene(BASIC_SCENE.into()), 42, 100)
            .is_ok()
    );
}

#[test]
fn unknown_scene_path_is_an_error_not_a_panic() {
    match engine_cli::verify_scenario_determinism(
        SimSource::Scene("tests/fixtures/scenes/does-not-exist.toml".into()),
        1,
        10,
    ) {
        Err(DeterminismResult::Error(e)) => assert_eq!(e.code, "SCENE_READ_ERROR"),
        Err(DeterminismResult::Mismatch(_)) => panic!("expected an error, got a mismatch"),
        Ok(_) => panic!("expected an error, got Ok"),
    }
}

/// Editing exactly one entity's one field in the scene file must change
/// exactly that entity's slice of the JSON output — nothing else.
#[test]
fn editing_one_entity_field_only_changes_that_entitys_json_subtree() {
    let before = engine_cli::run_and_dump(SimSource::Scene(BASIC_SCENE.into()), 1, 60).unwrap();
    let after =
        engine_cli::run_and_dump(SimSource::Scene(BASIC_SCENE_MODIFIED.into()), 1, 60).unwrap();

    assert_eq!(before["tick"], after["tick"]);
    assert_eq!(before["seed"], after["seed"]);

    let before_entities = before["entities"].as_array().unwrap();
    let after_entities = after["entities"].as_array().unwrap();
    assert_eq!(before_entities.len(), after_entities.len());

    let mut changed = Vec::new();
    for (b, a) in before_entities.iter().zip(after_entities.iter()) {
        let name = b["components"]["SceneName"].as_str().unwrap();
        assert_eq!(name, a["components"]["SceneName"].as_str().unwrap());
        if b != a {
            changed.push(name);
        }
    }

    assert_eq!(changed, vec!["mover-0"]);
}

#[test]
fn cli_run_subcommand_loads_scene_and_exits_successfully() {
    Command::cargo_bin("engine")
        .unwrap()
        .args(["run", BASIC_SCENE, "--ticks", "10", "--format", "json"])
        .assert()
        .success()
        .stdout(predicates::str::contains("\"status\":\"ok\""));
}

#[test]
fn cli_inspect_subcommand_supports_scene_flag() {
    Command::cargo_bin("engine")
        .unwrap()
        .args(["inspect", "--scene", BASIC_SCENE, "--ticks", "10"])
        .assert()
        .success()
        .stdout(predicates::str::contains("SceneName"));
}

#[test]
fn cli_test_subcommand_supports_scene_flag() {
    Command::cargo_bin("engine")
        .unwrap()
        .args([
            "test",
            "--scene",
            BASIC_SCENE,
            "--ticks",
            "10",
            "--format",
            "json",
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("\"status\":\"pass\""));
}

#[test]
fn cli_rejects_scenario_and_scene_together() {
    Command::cargo_bin("engine")
        .unwrap()
        .args(["test", "--scenario", "basic", "--scene", BASIC_SCENE])
        .assert()
        .failure();
}
