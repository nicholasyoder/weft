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
    #[serde(default, rename = "entity")]
    pub entities: Vec<EntityDef>,
    #[serde(default, rename = "system")]
    pub systems: Vec<SystemDef>,
}
