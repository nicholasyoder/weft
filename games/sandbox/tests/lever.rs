//! Proves `scripts/lever.lua` (see `src/gate.rs`, ADR-0018) actually works
//! in the real scene: `playground.toml`'s "gate" blocks the interior
//! partition, and "lever" opens it only when the player holds E within
//! range *and* `engine.raycast` reports a clear line of sight from the
//! lever to the player — a different interaction pattern from
//! `tests/pickup.rs`'s `engine.overlapping`-based proximity check. Same
//! "load the real scene through `sandbox::registry()`, step physics for
//! real, dispatch scripts directly" style as `tests/pickup.rs`/`tests/
//! hud.rs`.

use engine_core::sim::Sim;
use engine_core::{Input, KeyCode, Transform};
use engine_scene::{ComponentRegistry, SystemRegistry};
use engine_script::{DispatchCtx, Script, ScriptError, ScriptHost};
use sandbox::gate::Gate;
use sandbox::player_control::PlayerControl;

const SCENE: &str = "scenes/playground.toml";
// "lever" sits at [0.0, 0.6, -6.3]; comfortably inside lever.lua's
// MAX_DISTANCE (3.5) and with nothing in between.
const IN_RANGE_WITH_LINE_OF_SIGHT: glam::Vec3 = glam::Vec3::new(0.0, 0.6, -5.0);
// Far enough from "lever" (MAX_DISTANCE 3.5) that lever.lua's own distance
// check rejects it before any raycast is even attempted.
const OUT_OF_RANGE: glam::Vec3 = glam::Vec3::new(0.0, 0.6, 14.0);

fn load(
    components: &ComponentRegistry,
    systems: &SystemRegistry,
) -> (Sim, Vec<engine_core::inspect::ComponentDumper>) {
    engine_scene::load(SCENE.as_ref(), 1, components, systems).unwrap()
}

fn load_host() -> ScriptHost {
    let mut host = ScriptHost::new().unwrap();
    host.load_file("scripts/pickup.lua".as_ref()).unwrap();
    host.load_file("scripts/lever.lua".as_ref()).unwrap();
    host
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

fn move_player(sim: &mut Sim, position: glam::Vec3) {
    let player = sim
        .world
        .query::<&PlayerControl>()
        .iter()
        .next()
        .map(|(e, _)| e)
        .expect("playground.toml should have a PlayerControl entity");
    sim.world.get::<&mut Transform>(player).unwrap().position = position;
}

fn gate_count(world: &hecs::World) -> usize {
    world.query::<&Gate>().iter().count()
}

fn lever_script_count(world: &hecs::World) -> usize {
    world.query::<&Script>().iter().count()
}

#[test]
fn standing_far_from_the_lever_and_holding_e_leaves_the_gate_shut() {
    let (components, systems) = sandbox::registry();
    let (mut sim, dumpers) = load(&components, &systems);
    let mut host = load_host();

    assert_eq!(
        gate_count(&sim.world),
        1,
        "playground.toml should start with exactly one Gate-tagged entity"
    );

    move_player(&mut sim, OUT_OF_RANGE);
    sim.step().unwrap();

    let errors = dispatch(
        &mut sim,
        &mut host,
        &components,
        &dumpers,
        &held(KeyCode::E),
    );
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");
    assert_eq!(
        gate_count(&sim.world),
        1,
        "far from the lever: the gate should stay shut"
    );
}

#[test]
fn holding_e_near_the_lever_without_line_of_sight_leaves_the_gate_shut() {
    let (components, systems) = sandbox::registry();
    let (mut sim, dumpers) = load(&components, &systems);
    let mut host = load_host();

    // Within lever.lua's MAX_DISTANCE, but on the far side of the gate
    // itself (position [0.0, 1.0, -8.0]) from the lever (position
    // [0.0, 0.6, -6.3]) — the gate's own solid collider blocks the ray.
    move_player(&mut sim, glam::Vec3::new(0.0, 0.6, -9.5));
    sim.step().unwrap();

    let errors = dispatch(
        &mut sim,
        &mut host,
        &components,
        &dumpers,
        &held(KeyCode::E),
    );
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");
    assert_eq!(
        gate_count(&sim.world),
        1,
        "blocked line of sight: the gate should stay shut"
    );
}

#[test]
fn holding_e_in_range_with_line_of_sight_opens_the_gate_exactly_once() {
    let (components, systems) = sandbox::registry();
    let (mut sim, dumpers) = load(&components, &systems);
    let mut host = load_host();

    let scripted_before = lever_script_count(&sim.world);

    move_player(&mut sim, IN_RANGE_WITH_LINE_OF_SIGHT);
    sim.step().unwrap();

    let errors = dispatch(
        &mut sim,
        &mut host,
        &components,
        &dumpers,
        &held(KeyCode::E),
    );
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");
    assert_eq!(
        gate_count(&sim.world),
        0,
        "in range with a clear line of sight: the gate should open"
    );
    assert_eq!(
        lever_script_count(&sim.world),
        scripted_before - 1,
        "the lever should self-despawn as a one-shot switch"
    );

    // Dispatch again with E still held: the lever is already gone, so this
    // must be a no-op, not a panic/error over a missing entity.
    let errors = dispatch(
        &mut sim,
        &mut host,
        &components,
        &dumpers,
        &held(KeyCode::E),
    );
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");
    assert_eq!(gate_count(&sim.world), 0);
}
