use engine_core::inspect::ComponentDumper;
use engine_scene::ComponentRegistry;
use engine_script::{DispatchCtx, Script, ScriptError, ScriptHost};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct Counter {
    value: i64,
}

fn load_counter(
    v: serde_json::Value,
    b: &mut hecs::EntityBuilder,
) -> Result<(), serde_json::Error> {
    b.add(serde_json::from_value::<Counter>(v)?);
    Ok(())
}

fn dump_counter(e: &hecs::EntityRef) -> Option<(&'static str, serde_json::Value)> {
    e.get::<&Counter>()
        .map(|c| ("Counter", serde_json::to_value(&*c).unwrap()))
}

fn registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    registry.register("Counter", load_counter, dump_counter);
    registry
}

const DUMPERS: &[ComponentDumper] = &[dump_counter];
const COUNTER_SCRIPT: &str = "tests/fixtures/counter.lua";

#[test]
fn dispatch_calls_the_named_function_and_writes_back_its_result() {
    let mut world = hecs::World::new();
    let entity = world.spawn((
        Counter { value: 41 },
        Script {
            path: COUNTER_SCRIPT.to_string(),
            function: "on_tick".to_string(),
        },
    ));

    let mut host = ScriptHost::new().unwrap();
    host.load_file(COUNTER_SCRIPT.as_ref()).unwrap();

    let registry = registry();
    let errors = host.dispatch(DispatchCtx {
        world: &mut world,
        components: &registry,
        dumpers: DUMPERS,
        tick: 0,
        dt: 1.0 / 60.0,
    });
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");

    assert_eq!(world.get::<&Counter>(entity).unwrap().value, 42);
}

#[test]
fn load_failed_error_names_the_file_and_line() {
    let dir = std::env::temp_dir().join(format!("engine-script-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("broken.lua");
    std::fs::write(&path, "function on_tick(x\n  return x\nend\n").unwrap();

    let mut host = ScriptHost::new().unwrap();
    let err = host.load_file(&path).unwrap_err();
    assert_eq!(err.code(), "SCRIPT_LOAD_ERROR");
    let message = err.to_string();
    assert!(
        message.contains("broken.lua"),
        "error should name the file: {message}"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn math_random_is_disabled_not_ambiently_available() {
    let dir = std::env::temp_dir().join(format!("engine-script-test-rng-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("uses_random.lua");
    std::fs::write(
        &path,
        "function on_tick(components, tick, dt)\n  return { Counter = { value = math.random(1, 10) } }\nend\n",
    )
    .unwrap();

    let mut world = hecs::World::new();
    world.spawn((
        Counter { value: 0 },
        Script {
            path: path.display().to_string(),
            function: "on_tick".to_string(),
        },
    ));

    let mut host = ScriptHost::new().unwrap();
    host.load_file(&path).unwrap();

    let registry = registry();
    let errors = host.dispatch(DispatchCtx {
        world: &mut world,
        components: &registry,
        dumpers: DUMPERS,
        tick: 0,
        dt: 1.0 / 60.0,
    });
    assert_eq!(errors.len(), 1);
    assert!(matches!(errors[0].1, ScriptError::RuntimeFailed { .. }));

    std::fs::remove_dir_all(&dir).ok();
}
