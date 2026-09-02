//! Proves games/sandbox's own scripted content — not a synthetic
//! engine-script fixture — actually works: `scenes/playground.toml`'s three
//! gold pickups run `scripts/pickup.lua` (see ADR-0013), which uses
//! `engine.query`/`engine.key_held`/`engine.despawn` to let the player
//! collect a pickup by walking close and holding E. Loads the real scene
//! through the real `sandbox::registry()` and dispatches scripts directly —
//! no window needed, since dispatch itself doesn't require one (only
//! `engine-cli`'s live `play` loop, exercised separately by the `#[ignore]`d
//! subprocess test in `tests/play.rs`, needs a real windowing backend).

use engine_core::sim::Sim;
use engine_core::{Input, KeyCode, Transform};
use engine_scene::ComponentRegistry;
use engine_script::{DispatchCtx, Script, ScriptError, ScriptHost};
use sandbox::player_control::PlayerControl;

const SCENE: &str = "scenes/playground.toml";

fn pickup_count(world: &hecs::World) -> usize {
    world.query::<&Script>().iter().count()
}

fn dispatch(
    sim: &mut Sim,
    host: &mut ScriptHost,
    components: &ComponentRegistry,
    dumpers: &[engine_core::inspect::ComponentDumper],
    input: &Input,
) -> Vec<(hecs::Entity, ScriptError)> {
    host.dispatch(DispatchCtx {
        world: &mut sim.world,
        components,
        dumpers,
        rng: &mut sim.rng,
        input,
        resources: &mut sim.resources,
        tick: sim.tick,
        dt: sim.dt,
    })
}

fn held(key: KeyCode) -> Input {
    let mut input = Input::default();
    input.set_held(key, true);
    input
}

#[test]
fn walking_near_a_pickup_and_holding_e_collects_it() {
    let (components, systems) = sandbox::registry();
    let (mut sim, dumpers) = engine_scene::load(SCENE.as_ref(), 1, &components, &systems).unwrap();

    let mut paths: Vec<String> = sim
        .world
        .query::<&Script>()
        .iter()
        .map(|(_, s)| s.path.clone())
        .collect();
    paths.sort();
    paths.dedup();
    let mut host = ScriptHost::new().unwrap();
    for path in &paths {
        host.load_file(path.as_ref()).unwrap();
    }

    assert_eq!(
        pickup_count(&sim.world),
        3,
        "playground.toml should start with 3 scripted pickups"
    );

    let player = sim
        .world
        .query::<&PlayerControl>()
        .iter()
        .next()
        .map(|(e, _)| e)
        .expect("playground.toml should have a PlayerControl entity");

    // The player starts far from every pickup: holding E should collect
    // nothing.
    let errors = dispatch(
        &mut sim,
        &mut host,
        &components,
        &dumpers,
        &held(KeyCode::E),
    );
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");
    assert_eq!(
        pickup_count(&sim.world),
        3,
        "far away from every pickup: nothing should be collected"
    );

    // Move the player on top of pickup_1 (scene position [4.0, 0.4, 4.0])
    // and hold E: exactly one pickup should be collected.
    {
        let mut transform = sim.world.get::<&mut Transform>(player).unwrap();
        transform.position = glam::Vec3::new(4.0, 0.4, 4.0);
    }
    let errors = dispatch(
        &mut sim,
        &mut host,
        &components,
        &dumpers,
        &held(KeyCode::E),
    );
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");
    assert_eq!(
        pickup_count(&sim.world),
        2,
        "standing on a pickup with E held should collect exactly one"
    );

    // Dispatch again without E held: no further pickups should disappear,
    // even though the player is still standing on the same spot.
    let errors = dispatch(
        &mut sim,
        &mut host,
        &components,
        &dumpers,
        &Input::default(),
    );
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");
    assert_eq!(
        pickup_count(&sim.world),
        2,
        "releasing E should not collect anything else"
    );
}
