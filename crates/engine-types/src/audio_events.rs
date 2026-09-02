/// One `engine.play_sound(clip, volume)` Lua call queued for this tick
/// (see ADR-0016). Written by `engine-script`'s `dispatch_one` into a
/// `Resources`-held `SoundEventQueue`, drained by `engine-audio`'s
/// `audio_step` at the *start* of the following tick (script dispatch runs
/// after every native system, `audio_step` included, so a one-shot fired
/// this tick is always observed one tick later — the same category of lag
/// `games/sandbox`'s `hud_system` already accepts for post-physics reads).
/// Lives in `engine_core`, not `engine-script`/`engine-audio`, for the same
/// "producer and consumer need a shared ancestor crate" reason
/// `JointPalette` does (ADR-0015).
#[derive(Debug, Clone, PartialEq)]
pub struct SoundEvent {
    pub entity: hecs::Entity,
    pub clip: String,
    pub volume: f32,
}

/// Per-tick queue of `SoundEvent`s, held in a `Sim`'s `Resources` bag.
/// `engine-script` only ever pushes; `engine-audio`'s `audio_step` is the
/// sole drainer, via `std::mem::take`.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct SoundEventQueue(pub Vec<SoundEvent>);
