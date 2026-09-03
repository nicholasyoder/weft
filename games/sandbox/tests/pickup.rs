//! Proves games/sandbox's own scripted content — not a synthetic
//! engine-script fixture — actually works: `scenes/playground.toml`'s three
//! gold pickups run `scripts/pickup.lua` (see ADR-0013), which uses
//! `engine.overlapping`/`engine.key_held`/`engine.despawn` to let the player
//! collect a pickup by walking close (a real rapier sensor overlap, see
//! physics-substrate-plan.md Phase 6) and holding E. Loads the real scene
//! through the real `sandbox::registry()`, steps physics for real (needed
//! so rapier's narrow phase actually computes the sensor overlap), then
//! dispatches scripts directly — no window needed (only `engine-cli`'s live
//! `play` loop, exercised separately by the `#[ignore]`d subprocess test in
//! `tests/play.rs`, needs a real windowing backend).

use engine_core::sim::Sim;
use engine_core::{Input, KeyCode, Transform};
use engine_scene::{ComponentRegistry, SystemRegistry};
use engine_script::{DispatchCtx, Script, ScriptError, ScriptHost};
use sandbox::player_control::PlayerControl;

const SCENE: &str = "scenes/playground.toml";

fn pickup_count(world: &hecs::World) -> usize {
    world.query::<&Script>().iter().count()
}

fn load(
    components: &ComponentRegistry,
    systems: &SystemRegistry,
) -> (Sim, Vec<engine_core::inspect::ComponentDumper>) {
    engine_scene::load(SCENE.as_ref(), 1, components, systems).unwrap()
}

fn load_pickup_host() -> ScriptHost {
    let mut host = ScriptHost::new().unwrap();
    host.load_file("scripts/pickup.lua".as_ref()).unwrap();
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

#[test]
fn standing_far_from_every_pickup_and_holding_e_collects_nothing() {
    let (components, systems) = sandbox::registry();
    let (mut sim, dumpers) = load(&components, &systems);
    let mut host = load_pickup_host();

    assert_eq!(
        pickup_count(&sim.world),
        3,
        "playground.toml should start with 3 scripted pickups"
    );

    // A real physics tick registers the player's and every pickup's rapier
    // collider at their scene-authored (far apart) positions and computes
    // this tick's sensor-overlap state — nothing should be overlapping.
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
        pickup_count(&sim.world),
        3,
        "far away from every pickup: nothing should be collected"
    );
}

#[test]
fn standing_on_a_pickup_and_holding_e_collects_exactly_one() {
    let (components, systems) = sandbox::registry();
    let (mut sim, dumpers) = load(&components, &systems);
    let mut host = load_pickup_host();

    let player = sim
        .world
        .query::<&PlayerControl>()
        .iter()
        .next()
        .map(|(e, _)| e)
        .expect("playground.toml should have a PlayerControl entity");

    // Move the player onto pickup_1 (scene position [4.0, 0.4, 4.0])
    // *before* the very first physics tick. `physics_step` only reads an
    // entity's Transform to seed its rapier body at first registration — a
    // Transform mutated on an already-registered dynamic body has no
    // further effect, since its pose from then on is driven purely by
    // rapier's own simulation. Doing this pre-registration is exactly the
    // real-gameplay case anyway (the player literally rolls there under
    // WASD/physics before ever pressing E); it's also how
    // `overlapping_reports_sensor_overlaps_and_updates_when_bodies_move_apart`
    // (`crates/engine-physics/src/queries.rs`) proves rapier's narrow phase
    // sees an overlap for two bodies registered already-overlapping on their
    // very first step.
    {
        let mut transform = sim.world.get::<&mut Transform>(player).unwrap();
        transform.position = glam::Vec3::new(4.0, 0.4, 4.0);
    }

    sim.step().unwrap(); // registers both bodies already overlapping and computes this tick's overlap

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
