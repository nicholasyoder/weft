use serde::{Deserialize, Serialize};

fn default_true() -> bool {
    true
}

fn default_volume() -> f32 {
    1.0
}

/// Scene-authored, steady-state looping audio — typically background
/// music. Mirrors `engine_anim::Animator`'s shape: started once per entity
/// by `audio_step` (tracked internally, keyed by entity), stopped when the
/// entity despawns or `playing` goes false. One-shot SFX use
/// `engine.play_sound` instead (see ADR-0016) — this component is
/// deliberately not used for one-shots.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioSource {
    pub clip: String,
    #[serde(default = "default_true")]
    pub playing: bool,
    #[serde(default = "default_true")]
    pub looping: bool,
    #[serde(default = "default_volume")]
    pub volume: f32,
}

/// Which clips fired via `engine.play_sound` on this entity *this tick*
/// (see ADR-0016) — written and cleared every tick by `audio_step`, purely
/// so batch commands (`test`/`inspect`/`run`/`replay`, which never open a
/// real audio device) have something observable about one-shot triggers.
/// Never scene-authored in practice, but registered with a real loader
/// anyway — same "harmless initial value, overwritten next tick" posture
/// `JointPalette` established (ADR-0015).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SoundsPlayed {
    pub clips: Vec<String>,
}
