use std::collections::HashMap;
use std::io::Cursor;

use engine_assets::AssetStore;
use engine_core::scheduler::SystemArgs;
use engine_core::{AssetsDir, AudioSettings, SoundEventQueue};
use kira::sound::static_sound::{StaticSoundData, StaticSoundHandle};
use kira::Tween;

use crate::backend::AudioBackend;
use crate::components::{AudioSource, SoundsPlayed};

/// Lazily-decoded audio clips, held alongside per-entity "have I started
/// this `AudioSource` yet" tracking — the same "cache keyed by content
/// hash, decoded once, reused every tick" shape `engine-anim`'s
/// `AnimCache`/`engine-render`'s `*_cache` fields already use (see
/// ADR-0015/0016). Private: `engine-cli` only ever touches `AudioState`,
/// never this directly.
#[derive(Default)]
struct AudioCache {
    clips: HashMap<String, StaticSoundData>,
    /// `None` once started with no live handle to stop later (batch
    /// commands with no backend, or the offline `Mixdown`, which has no
    /// per-voice handle concept at all); `Some` only for the live backend.
    started: HashMap<hecs::Entity, Option<StaticSoundHandle>>,
}

impl AudioCache {
    /// Decodes and caches the clip at `hash` if not already cached. An
    /// unresolvable or corrupt hash panics with a clear message — this is
    /// scene-authored/scripted data reaching a system with no
    /// `Result`-returning path (`System = fn(&mut SystemArgs)`), the same
    /// posture `AnimCache::ensure_clip`/`ensure_skeleton` already
    /// establish and explain in detail.
    fn ensure_clip(&mut self, store: &AssetStore, hash: &str) {
        if self.clips.contains_key(hash) {
            return;
        }
        let bytes = store.get(hash).unwrap_or_else(|e| {
            panic!("AudioSource/engine.play_sound references unknown clip '{hash}': {e}")
        });
        let decoded = StaticSoundData::from_cursor(Cursor::new(bytes))
            .unwrap_or_else(|e| panic!("failed to decode audio clip '{hash}': {e}"));
        self.clips.insert(hash.to_string(), decoded);
    }

    fn clip(&self, hash: &str) -> &StaticSoundData {
        self.clips
            .get(hash)
            .expect("ensure_clip must be called before clip")
    }

    fn is_started(&self, entity: hecs::Entity) -> bool {
        self.started.contains_key(&entity)
    }

    fn mark_started(&mut self, entity: hecs::Entity, handle: Option<StaticSoundHandle>) {
        self.started.insert(entity, handle);
    }

    /// Mirrors `engine-physics`'s `evict_despawned`/ADR-0011 exactly:
    /// stops and forgets any tracked entity no longer present in `world`,
    /// in `Entity::to_bits()` order.
    fn evict_despawned(&mut self, world: &hecs::World) {
        let mut stale: Vec<hecs::Entity> = self
            .started
            .keys()
            .copied()
            .filter(|e| !world.contains(*e))
            .collect();
        stale.sort_by_key(|e| e.to_bits());
        for entity in stale {
            if let Some(Some(mut handle)) = self.started.remove(&entity) {
                handle.stop(Tween::default());
            }
        }
    }
}

/// The one `Resources` entry `audio_step` reads/writes. `backend` is
/// `None` for every batch command (`test`/`inspect`/`run`/`replay`) —
/// `cache` is private (mirrors `engine-physics::PhysicsState`'s
/// public-`world`-private-`bodies` shape).
#[derive(Default)]
pub struct AudioState {
    pub backend: Option<AudioBackend>,
    cache: AudioCache,
}

impl AudioState {
    pub fn with_backend(backend: AudioBackend) -> Self {
        Self {
            backend: Some(backend),
            cache: AudioCache::default(),
        }
    }
}

/// Drains `engine.play_sound` one-shots, starts any not-yet-started
/// `AudioSource` loop, evicts despawned entities' tracked voices, and (for
/// the offline `Mix` backend) advances the mixdown by `args.dt`.
/// Registered into `SystemRegistry` as `"audio"`.
///
/// Two things happen unconditionally, even with no `AssetsDir`/`AudioState`
/// at all: draining the event queue (so it never grows unbounded) and
/// writing a dumpable `SoundsPlayed` snapshot per entity — this is what
/// lets batch commands observe "which sounds fired" without ever opening a
/// real device (see ADR-0016).
pub fn audio_step(args: &mut SystemArgs) {
    let events = std::mem::take(
        &mut args
            .resources
            .get_or_insert_with(SoundEventQueue::default)
            .0,
    );
    apply_sounds_played(args.world, &events);

    let Some(assets_dir) = args.resources.get::<AssetsDir>().map(|a| a.0.clone()) else {
        return;
    };
    let settings = args
        .resources
        .get::<AudioSettings>()
        .copied()
        .unwrap_or_default();
    let store = AssetStore::new(assets_dir);
    let state = args.resources.get_or_insert_with(AudioState::default);

    state.cache.evict_despawned(args.world);

    for event in &events {
        state.cache.ensure_clip(&store, &event.clip);
        let clip = state.cache.clip(&event.clip);
        if let Some(backend) = &mut state.backend {
            backend.play_one_shot(clip, event.volume * settings.sfx * settings.master);
        }
    }

    let sources: Vec<(hecs::Entity, AudioSource)> = args
        .world
        .query::<&AudioSource>()
        .iter()
        .map(|(e, s)| (e, s.clone()))
        .collect();
    for (entity, source) in sources {
        if state.cache.is_started(entity) || !source.playing {
            continue;
        }
        state.cache.ensure_clip(&store, &source.clip);
        let clip = state.cache.clip(&source.clip);
        let handle = state.backend.as_mut().and_then(|backend| {
            backend.play_source(
                clip,
                source.volume * settings.music * settings.master,
                source.looping,
            )
        });
        state.cache.mark_started(entity, handle);
    }

    if let Some(AudioBackend::Mix(mixdown)) = &mut state.backend {
        mixdown.render(args.dt);
    }
}

fn apply_sounds_played(world: &mut hecs::World, events: &[engine_core::SoundEvent]) {
    let mut played_by_entity: HashMap<hecs::Entity, Vec<String>> = HashMap::new();
    for event in events {
        played_by_entity
            .entry(event.entity)
            .or_default()
            .push(event.clip.clone());
    }

    let stale: Vec<hecs::Entity> = world
        .query::<&SoundsPlayed>()
        .iter()
        .map(|(e, _)| e)
        .filter(|e| !played_by_entity.contains_key(e))
        .collect();
    for entity in stale {
        let _ = world.remove_one::<SoundsPlayed>(entity);
    }
    for (entity, clips) in played_by_entity {
        let _ = world.insert_one(entity, SoundsPlayed { clips });
    }
}
