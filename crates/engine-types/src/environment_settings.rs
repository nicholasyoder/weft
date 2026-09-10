/// Scene-wide hemisphere ambient lighting, carried as a `Resources` entry
/// (see `Resources`, ADR-0008's precedent). Populated from a scene file's
/// `[environment]` table by `engine_scene::load` (defaulting to a mild
/// cool-sky/warm-ground pair for scenes/scenarios that don't author one) —
/// lives here rather than in `engine-scene` or `engine-render` because it's
/// produced by scene-loading and consumed by `engine-render`'s ambient
/// term, the same producer/consumer relationship `AudioSettings` already
/// has with `engine-audio`.
///
/// This is a single-color-per-hemisphere approximation, not prefiltered/
/// roughness-aware image-based lighting — see the ambient-lighting ADR for
/// why a full cubemap/environment-texture approach was scoped out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EnvironmentSettings {
    /// Ambient color for surfaces facing "up" (`+Y`).
    pub sky_color: [f32; 3],
    /// Ambient color for surfaces facing "down" (`-Y`).
    pub ground_color: [f32; 3],
    /// Overall ambient intensity multiplier.
    pub intensity: f32,
}

impl Default for EnvironmentSettings {
    fn default() -> Self {
        Self {
            sky_color: [0.35, 0.45, 0.6],
            ground_color: [0.25, 0.22, 0.18],
            intensity: 0.35,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_a_mild_cool_sky_warm_ground_pair() {
        assert_eq!(
            EnvironmentSettings::default(),
            EnvironmentSettings {
                sky_color: [0.35, 0.45, 0.6],
                ground_color: [0.25, 0.22, 0.18],
                intensity: 0.35,
            }
        );
    }
}
