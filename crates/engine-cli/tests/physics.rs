use engine_cli::SimSource;

const PHYSICS_SCENE: &str = "tests/fixtures/scenes/physics_demo.toml";

fn ball_position_y(json: &serde_json::Value) -> f64 {
    let entities = json["entities"].as_array().unwrap();
    let ball = entities
        .iter()
        .find(|e| e["components"]["SceneName"] == "ball")
        .expect("scene has a \"ball\" entity");
    ball["components"]["Transform"]["position"][1]
        .as_f64()
        .unwrap()
}

#[test]
fn ball_falls_under_gravity() {
    let after_a_few_ticks =
        engine_cli::run_and_dump(SimSource::Scene(PHYSICS_SCENE.into()), 1, 5).unwrap();
    // Started at y=5.0 in the fixture.
    assert!(
        ball_position_y(&after_a_few_ticks) < 5.0,
        "expected the ball to have fallen after 5 ticks"
    );
}

#[test]
fn ball_comes_to_rest_on_the_ground_plane() {
    let after_resting =
        engine_cli::run_and_dump(SimSource::Scene(PHYSICS_SCENE.into()), 1, 300).unwrap();
    let y = ball_position_y(&after_resting);
    // Ground top is at y=0.1 (half-extent 0.1), ball radius 0.5: rest ~= 0.6.
    assert!(
        (y - 0.6).abs() < 0.05,
        "expected the ball to be resting around y=0.6, got y={y}"
    );
}

#[test]
fn physics_scene_is_deterministic() {
    assert!(engine_cli::verify_scenario_determinism(
        SimSource::Scene(PHYSICS_SCENE.into()),
        7,
        300
    )
    .is_ok());
}

#[test]
fn physics_demo_scenario_is_deterministic() {
    assert!(engine_cli::verify_scenario_determinism("physics-demo", 7, 300).is_ok());
}
