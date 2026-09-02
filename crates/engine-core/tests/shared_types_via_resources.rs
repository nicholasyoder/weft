//! `AssetsDir`/`AudioSettings`/`SoundEventQueue` are defined in
//! `engine-types` (see `docs/roadmap/debt-cleanup-plan.md`'s Phase 3), a
//! leaf crate that can't depend on `Resources` — so their "does this
//! particular type actually round-trip through a Sim's `Resources` bag"
//! coverage lives here instead, on the `engine_core` re-export, alongside
//! the crate that owns `Resources` itself.

use engine_core::{AssetsDir, AudioSettings, Resources, SoundEvent, SoundEventQueue};

#[test]
fn assets_dir_round_trips_through_resources() {
    let mut resources = Resources::new();
    resources.insert(AssetsDir(std::path::PathBuf::from("assets")));
    assert_eq!(
        resources.get::<AssetsDir>(),
        Some(&AssetsDir(std::path::PathBuf::from("assets")))
    );
}

#[test]
fn audio_settings_round_trips_through_resources() {
    let mut resources = Resources::new();
    resources.insert(AudioSettings {
        master: 0.5,
        music: 0.8,
        sfx: 1.0,
    });
    assert_eq!(
        resources.get::<AudioSettings>(),
        Some(&AudioSettings {
            master: 0.5,
            music: 0.8,
            sfx: 1.0,
        })
    );
}

#[test]
fn sound_event_queue_drains_via_mem_take_leaving_an_empty_queue_behind() {
    let mut world = hecs::World::new();
    let entity = world.spawn(());
    let mut resources = Resources::new();
    resources
        .get_or_insert_with(SoundEventQueue::default)
        .0
        .push(SoundEvent {
            entity,
            clip: "abc".to_string(),
            volume: 1.0,
        });

    let drained = std::mem::take(&mut resources.get_mut::<SoundEventQueue>().unwrap().0);
    assert_eq!(drained.len(), 1);
    assert_eq!(resources.get::<SoundEventQueue>().unwrap().0.len(), 0);
}
