use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
pub(crate) struct Meta {
    pub dt: Option<f32>,
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
    #[serde(default, rename = "entity")]
    pub entities: Vec<EntityDef>,
    #[serde(default, rename = "system")]
    pub systems: Vec<SystemDef>,
}
