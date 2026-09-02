use serde::{Deserialize, Serialize};

/// Drives skeletal playback for an entity: which skeleton/clip asset (by
/// content hash) to sample, and simple play/loop/speed state. Scene-
/// authorable — see `crate::system::animation_step`, which advances `time`
/// every tick and writes the resulting `JointPalette`. Deliberately one
/// clip at a time, no blending/crossfade — see ADR-0015's "revisit when".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Animator {
    pub skeleton: String,
    pub clip: String,
    #[serde(default = "default_true")]
    pub playing: bool,
    #[serde(default = "default_true")]
    pub looping: bool,
    #[serde(default = "default_speed")]
    pub speed: f32,
    /// Seconds into the clip; authorable as a start offset, otherwise
    /// mutated in place by `animation_step` every tick.
    #[serde(default)]
    pub time: f32,
}

fn default_true() -> bool {
    true
}

fn default_speed() -> f32 {
    1.0
}
