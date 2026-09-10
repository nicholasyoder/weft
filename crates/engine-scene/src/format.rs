use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
pub(crate) struct Meta {
    pub dt: Option<f32>,
}

fn default_volume() -> f32 {
    1.0
}

/// Scene-authorable mixer group volumes (see ADR-0016) — a fourth
/// top-level table alongside `meta`/`entity`/`system`, mirroring `Meta`'s
/// own "all fields optional, sensible defaults" shape so every existing
/// scene file keeps working unchanged (implicit full volume).
#[derive(Debug, Deserialize)]
pub(crate) struct AudioMeta {
    #[serde(default = "default_volume")]
    pub master: f32,
    #[serde(default = "default_volume")]
    pub music: f32,
    #[serde(default = "default_volume")]
    pub sfx: f32,
}

impl Default for AudioMeta {
    fn default() -> Self {
        Self {
            master: default_volume(),
            music: default_volume(),
            sfx: default_volume(),
        }
    }
}

fn default_sky_color() -> [f32; 3] {
    [0.35, 0.45, 0.6]
}

fn default_ground_color() -> [f32; 3] {
    [0.25, 0.22, 0.18]
}

fn default_ambient_intensity() -> f32 {
    0.35
}

/// Scene-authorable hemisphere ambient lighting (see the ambient-lighting
/// ADR) — a fifth top-level table alongside `meta`/`audio`/`entity`/
/// `system`, mirroring `AudioMeta`'s own "all fields optional, sensible
/// defaults" shape. Defaults are literal-duplicated from
/// `engine_types::EnvironmentSettings::default()` (same stylistic
/// convention `AudioMeta`/`AudioSettings` already established) — keep the
/// two in sync if either is retuned.
#[derive(Debug, Deserialize)]
pub(crate) struct EnvironmentMeta {
    #[serde(default = "default_sky_color")]
    pub sky_color: [f32; 3],
    #[serde(default = "default_ground_color")]
    pub ground_color: [f32; 3],
    #[serde(default = "default_ambient_intensity")]
    pub intensity: f32,
}

impl Default for EnvironmentMeta {
    fn default() -> Self {
        Self {
            sky_color: default_sky_color(),
            ground_color: default_ground_color(),
            intensity: default_ambient_intensity(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct EntityDef {
    pub name: String,
    #[serde(default)]
    pub components: BTreeMap<String, toml::Value>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SystemDef {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SceneDef {
    #[serde(default)]
    pub meta: Meta,
    #[serde(default)]
    pub audio: AudioMeta,
    #[serde(default)]
    pub environment: EnvironmentMeta,
    #[serde(default, rename = "entity")]
    pub entities: Vec<EntityDef>,
    #[serde(default, rename = "system")]
    pub systems: Vec<SystemDef>,
}
