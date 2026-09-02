//! Proves `playground.toml`'s "ambience" looping `AudioSource` works in a
//! real game scene, not just a synthetic fixture — and specifically that an
//! unrelated entity's despawn doesn't disturb it, the exact class of bug
//! fixed in commit b497819 ("Fix engine mix/play divergence on despawned
//! looping audio"). Like `animation.rs`, `audio_step` silently no-ops
//! without an `AssetsDir` resource, so this test inserts one itself.

use std::path::PathBuf;

use engine_audio::AudioSource;
use engine_core::AssetsDir;
use sandbox::hud::Pickup;

const SCENE: &str = "scenes/playground.toml";

#[test]
fn ambient_loop_survives_ticks_and_an_unrelated_despawn() {
    let (components, systems) = sandbox::registry();
    let (mut sim, _dumpers) = engine_scene::load(SCENE.as_ref(), 1, &components, &systems).unwrap();
    sim.resources.insert(AssetsDir(PathBuf::from("assets")));

    let ambience = sim
        .world
        .query::<&AudioSource>()
        .iter()
        .next()
        .map(|(e, _)| e)
        .expect("playground.toml should have one AudioSource entity");

    for _ in 0..5 {
        sim.step()
            .expect("audio_step should resolve/decode the ambient clip cleanly");
    }

    // Despawn an unrelated entity directly (no need for full Lua script
    // dispatch — mirrors engine-physics's own
    // despawning_an_entity_evicts_its_physics_body-style directness).
    let pickup = sim
        .world
        .query::<&Pickup>()
        .iter()
        .next()
        .map(|(e, _)| e)
        .expect("playground.toml should have at least one Pickup entity");
    sim.world.despawn(pickup).unwrap();

    for _ in 0..5 {
        sim.step()
            .expect("audio_step should keep succeeding after an unrelated entity despawns");
    }

    assert!(
        sim.world.get::<&AudioSource>(ambience).is_ok(),
        "the ambient AudioSource entity should still exist, untouched by an unrelated despawn"
    );
}
