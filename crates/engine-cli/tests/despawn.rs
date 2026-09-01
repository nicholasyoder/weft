use engine_cli::SimSource;

const DESPAWN_SCENE: &str = "tests/fixtures/scenes/despawn_demo.toml";

fn entity_names(json: &serde_json::Value) -> Vec<String> {
    json["entities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["components"]["SceneName"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn marker_entity_is_still_present_before_its_countdown_expires() {
    let json = engine_cli::run_and_dump(SimSource::Scene(DESPAWN_SCENE.into()), 1, 2).unwrap();
    assert!(entity_names(&json).contains(&"marker".to_string()));
}

#[test]
fn marker_entity_is_gone_after_its_countdown_expires() {
    let json = engine_cli::run_and_dump(SimSource::Scene(DESPAWN_SCENE.into()), 1, 3).unwrap();
    assert!(!entity_names(&json).contains(&"marker".to_string()));
}

#[test]
fn physics_attached_entity_is_gone_after_its_countdown_expires() {
    let json = engine_cli::run_and_dump(SimSource::Scene(DESPAWN_SCENE.into()), 1, 10).unwrap();
    let names = entity_names(&json);
    assert!(!names.contains(&"ball".to_string()));
    assert!(names.contains(&"ground".to_string()));
}

#[test]
fn despawn_scene_is_deterministic() {
    assert!(
        engine_cli::verify_scenario_determinism(SimSource::Scene(DESPAWN_SCENE.into()), 7, 10)
            .is_ok()
    );
}

#[test]
fn despawn_demo_scenario_is_deterministic() {
    assert!(engine_cli::verify_scenario_determinism("despawn-demo", 7, 10).is_ok());
}
