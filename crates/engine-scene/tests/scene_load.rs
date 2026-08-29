use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use engine_core::inspect::world_to_json;
use engine_core::scheduler::SystemArgs;
use engine_scene::{ComponentRegistry, SystemRegistry};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Position {
    x: f32,
    y: f32,
}

fn load_position(
    v: serde_json::Value,
    b: &mut hecs::EntityBuilder,
) -> Result<(), serde_json::Error> {
    b.add(serde_json::from_value::<Position>(v)?);
    Ok(())
}

fn dump_position(e: &hecs::EntityRef) -> Option<(&'static str, serde_json::Value)> {
    e.get::<&Position>()
        .map(|p| ("Position", serde_json::to_value(&*p).unwrap()))
}

fn nudge(args: &mut SystemArgs) {
    for (_e, pos) in args.world.query::<&mut Position>().iter() {
        pos.x += 1.0;
    }
}

fn registries() -> (ComponentRegistry, SystemRegistry) {
    let mut components = ComponentRegistry::new();
    components.register("Position", load_position, dump_position);
    let mut systems = SystemRegistry::new();
    systems.register("nudge", nudge);
    (components, systems)
}

/// Writes `contents` to a uniquely-named scratch file and returns its path.
/// Left on disk in the OS temp dir rather than cleaned up — these are tiny
/// text fixtures, not worth RAII ceremony for a test helper.
fn write_fixture(contents: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path =
        std::env::temp_dir().join(format!("engine-scene-test-{}-{n}.toml", std::process::id()));
    std::fs::write(&path, contents).unwrap();
    path
}

#[test]
fn loads_entities_in_file_order_and_runs_systems() {
    let path = write_fixture(
        r#"
        [[entity]]
        name = "a"
        [entity.components.Position]
        x = 0.0
        y = 0.0

        [[entity]]
        name = "b"
        [entity.components.Position]
        x = 10.0
        y = 0.0

        [[system]]
        name = "nudge"
        "#,
    );
    let (components, systems) = registries();
    let (mut sim, dumpers) = engine_scene::load(&path, 1, &components, &systems).unwrap();
    sim.run(1);

    let json = world_to_json(&sim.world, sim.tick, 1, &dumpers);
    let entities = json["entities"].as_array().unwrap();
    assert_eq!(entities.len(), 2);
    assert_eq!(entities[0]["components"]["SceneName"], "a");
    assert_eq!(entities[0]["components"]["Position"]["x"], 1.0);
    assert_eq!(entities[1]["components"]["SceneName"], "b");
    assert_eq!(entities[1]["components"]["Position"]["x"], 11.0);
}

#[test]
fn meta_dt_defaults_to_sixtieth_of_a_second() {
    let path = write_fixture("");
    let (components, systems) = registries();
    let (sim, _) = engine_scene::load(&path, 1, &components, &systems).unwrap();
    assert_eq!(sim.dt, 1.0 / 60.0);
}

#[test]
fn unknown_component_is_a_structured_error_not_a_panic() {
    let path = write_fixture(
        r#"
        [[entity]]
        name = "a"
        [entity.components.NoSuchComponent]
        x = 0.0
        "#,
    );
    let (components, systems) = registries();
    let err = match engine_scene::load(&path, 1, &components, &systems) {
        Err(e) => e,
        Ok(_) => panic!("expected an error"),
    };
    assert_eq!(err.code(), "SCENE_UNKNOWN_COMPONENT");
}

#[test]
fn unknown_system_is_a_structured_error_not_a_panic() {
    let path = write_fixture(
        r#"
        [[system]]
        name = "no-such-system"
        "#,
    );
    let (components, systems) = registries();
    let err = match engine_scene::load(&path, 1, &components, &systems) {
        Err(e) => e,
        Ok(_) => panic!("expected an error"),
    };
    assert_eq!(err.code(), "SCENE_UNKNOWN_SYSTEM");
}
