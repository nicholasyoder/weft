/// Mixer group volumes for a `Sim`, carried as a `Resources` entry (see
/// `Resources`, ADR-0008's precedent). Populated from a scene file's
/// `[audio]` table by `engine_scene::load` (defaulting to `1.0`/`1.0`/`1.0`
/// for scenes/scenarios that don't author one) — lives here rather than in
/// `engine-scene` or `engine-audio` because it's produced by scene-loading
/// and consumed by `engine-audio`'s `audio_step`, the same
/// producer/consumer relationship `Transform`/`JointPalette` already have
/// with `engine-physics`/`engine-anim` (see ADR-0015, ADR-0016).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AudioSettings {
    pub master: f32,
    pub music: f32,
    pub sfx: f32,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            master: 1.0,
            music: 1.0,
            sfx: 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_full_volume() {
        assert_eq!(
            AudioSettings::default(),
            AudioSettings {
                master: 1.0,
                music: 1.0,
                sfx: 1.0,
            }
        );
    }
}
