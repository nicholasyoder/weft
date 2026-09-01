use serde::{Deserialize, Serialize};

/// A data-only reference to a Lua content script attached to an entity:
/// `path` is loaded once per distinct file, `function` is the global Lua
/// function `ScriptHost::dispatch` calls every tick for entities carrying
/// this component. No behavior lives here — this is exactly the kind of
/// opaque component the scene-file registry already knows how to carry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Script {
    pub path: String,
    pub function: String,
}
