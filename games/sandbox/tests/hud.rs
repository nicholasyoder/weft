//! Proves `hud_system` (see `src/hud.rs`, ADR-0014) actually reflects real
//! gameplay state, not just that it compiles: `playground.toml`'s `hud`
//! entity should read "Pickups: 0/3" before anything is collected and
//! "Pickups: 1/3" one tick after a real pickup is collected via
//! `scripts/pickup.lua` — the same "load the real scene through
//! `sandbox::registry()` and dispatch for real" style `tests/pickup.rs`
//! already uses, extended to also step the scene's `[[system]]` list
//! (`hud_system` is a plain system, not a script).

use engine_core::sim::Sim;
use engine_core::{Input, KeyCode, Transform};
use engine_render::Text;
use engine_scene::ComponentRegistry;
use engine_script::{DispatchCtx, ScriptError, ScriptHost};
use sandbox::player_control::PlayerControl;

const SCENE: &str = "scenes/playground.toml";

fn hud_text(world: &hecs::World) -> String {
    world
        .query::<&Text>()
        .iter()
        .next()
        .map(|(_, t)| t.content.clone())
        .expect("playground.toml should have a Text (hud) entity")
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
fn hud_text_reflects_collected_pickups() {
    use sandbox::hud::Pickup;

    let (components, systems) = sandbox::registry();
    let (mut sim, dumpers) = engine_scene::load(SCENE.as_ref(), 1, &components, &systems).unwrap();

    let mut paths: Vec<String> = sim
        .world
        .query::<&engine_script::Script>()
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
        sim.world.query::<&Pickup>().iter().count(),
        3,
        "playground.toml should start with 3 Pickup-tagged entities"
    );

    // One system step with nothing collected yet: the counter should read
    // its starting value.
    sim.step();
    assert_eq!(hud_text(&sim.world), "Pickups: 0/3");

    // Move the player onto pickup_1 (scene position [4.0, 0.4, 4.0]) and
    // dispatch the script with E held: this despawns pickup_1 (and its
    // Pickup marker with it).
    {
        let player = sim
            .world
            .query::<&PlayerControl>()
            .iter()
            .next()
            .map(|(e, _)| e)
            .expect("playground.toml should have a PlayerControl entity");
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
    assert_eq!(sim.world.query::<&Pickup>().iter().count(), 2);

    // hud_system runs as part of the scene's [[system]] list — one more
    // step should pick up the despawn that just happened.
    sim.step();
    assert_eq!(hud_text(&sim.world), "Pickups: 1/3");
}
