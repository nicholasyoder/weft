use engine_core::inspect::ComponentDumper;
use engine_core::rng::EngineRng;
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
    let mut rng = engine_core::rng::seeded(1);
    let errors = host.dispatch(DispatchCtx {
        world: &mut world,
        components: &registry,
        dumpers: DUMPERS,
        rng: &mut rng,
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
    let mut rng = engine_core::rng::seeded(1);
    let errors = host.dispatch(DispatchCtx {
        world: &mut world,
        components: &registry,
        dumpers: DUMPERS,
        rng: &mut rng,
        tick: 0,
        dt: 1.0 / 60.0,
    });
    assert_eq!(errors.len(), 1);
    assert!(matches!(errors[0].1, ScriptError::RuntimeFailed { .. }));

    std::fs::remove_dir_all(&dir).ok();
}

fn write_script(dir: &std::path::Path, name: &str, src: &str) -> std::path::PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, src).unwrap();
    path
}

fn dispatch_once(
    world: &mut hecs::World,
    registry: &ComponentRegistry,
    rng: &mut EngineRng,
    host: &mut ScriptHost,
    tick: u64,
) -> Vec<(hecs::Entity, ScriptError)> {
    host.dispatch(DispatchCtx {
        world,
        components: registry,
        dumpers: DUMPERS,
        rng,
        tick,
        dt: 1.0 / 60.0,
    })
}

#[test]
fn engine_random_is_deterministic_for_the_same_seed_and_varies_for_different_seeds() {
    let dir = std::env::temp_dir().join(format!("engine-script-test-erand-{}", std::process::id()));
    let path = write_script(
        &dir,
        "uses_engine_random.lua",
        "function on_tick(components, tick, dt)\n  return { Counter = { value = engine.random_int(1, 1000000) } }\nend\n",
    );
    let registry = registry();

    let run = |seed: u64| -> i64 {
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
        let mut rng = engine_core::rng::seeded(seed);
        let errors = dispatch_once(&mut world, &registry, &mut rng, &mut host, 0);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        let value = world.query::<&Counter>().iter().next().unwrap().1.value;
        value
    };

    let a1 = run(7);
    let a2 = run(7);
    let b = run(8);
    assert_eq!(a1, a2, "same seed should draw the same value");
    assert_ne!(
        a1, b,
        "different seeds should (almost always) draw different values"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn engine_despawn_with_no_args_despawns_self_and_skips_write_back() {
    let dir =
        std::env::temp_dir().join(format!("engine-script-test-despawn-{}", std::process::id()));
    let path = write_script(
        &dir,
        "self_despawn.lua",
        "function on_tick(components, tick, dt)\n  engine.despawn()\n  return { Counter = { value = 999 } }\nend\n",
    );

    let mut world = hecs::World::new();
    let entity = world.spawn((
        Counter { value: 1 },
        Script {
            path: path.display().to_string(),
            function: "on_tick".to_string(),
        },
    ));

    let registry = registry();
    let mut host = ScriptHost::new().unwrap();
    host.load_file(&path).unwrap();
    let mut rng = engine_core::rng::seeded(1);
    let errors = dispatch_once(&mut world, &registry, &mut rng, &mut host, 0);
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");
    assert!(
        !world.contains(entity),
        "self-despawned entity should be gone"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn engine_despawn_by_id_removes_another_entity_found_via_query() {
    let dir = std::env::temp_dir().join(format!(
        "engine-script-test-despawnid-{}",
        std::process::id()
    ));
    let path = write_script(
        &dir,
        "despawn_target.lua",
        r#"
function on_tick(components, tick, dt, self_id)
  local targets = engine.query({"Counter"})
  for _, t in ipairs(targets) do
    if t.id ~= self_id and t.Counter.value == 5 then
      engine.despawn(t.id)
    end
  end
  return nil
end
"#,
    );

    let mut world = hecs::World::new();
    let controller = world.spawn((
        Counter { value: 0 },
        Script {
            path: path.display().to_string(),
            function: "on_tick".to_string(),
        },
    ));
    let target = world.spawn((Counter { value: 5 },));
    let bystander = world.spawn((Counter { value: 6 },));

    let registry = registry();
    let mut host = ScriptHost::new().unwrap();
    host.load_file(&path).unwrap();
    let mut rng = engine_core::rng::seeded(1);
    let errors = dispatch_once(&mut world, &registry, &mut rng, &mut host, 0);
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");

    assert!(world.contains(controller));
    assert!(
        !world.contains(target),
        "value-5 entity should be despawned"
    );
    assert!(
        world.contains(bystander),
        "value-6 entity should be untouched"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn engine_query_filters_by_required_component_names() {
    let dir = std::env::temp_dir().join(format!("engine-script-test-query-{}", std::process::id()));
    let path = write_script(
        &dir,
        "counts_matches.lua",
        r#"
function on_tick(components, tick, dt)
  local matches = engine.query({"Counter"})
  return { Counter = { value = #matches } }
end
"#,
    );

    let mut world = hecs::World::new();
    world.spawn((
        Counter { value: 0 },
        Script {
            path: path.display().to_string(),
            function: "on_tick".to_string(),
        },
    ));
    world.spawn((Counter { value: 1 },));
    world.spawn((Counter { value: 2 },));
    world.spawn(()); // no Counter — must not match

    let registry = registry();
    let mut host = ScriptHost::new().unwrap();
    host.load_file(&path).unwrap();
    let mut rng = engine_core::rng::seeded(1);
    let errors = dispatch_once(&mut world, &registry, &mut rng, &mut host, 0);
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");

    // 3 entities carry Counter: the scripted one (still 0 at dispatch time),
    // plus the two plain ones.
    let mut binding = world.query::<(&Counter, &Script)>();
    let (_, (counter, _)) = binding.iter().next().unwrap();
    assert_eq!(counter.value, 3);

    std::fs::remove_dir_all(&dir).ok();
}
