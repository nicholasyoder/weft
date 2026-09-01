use std::collections::HashMap;
use std::path::{Path, PathBuf};

use engine_core::inspect::ComponentDumper;
use engine_scene::ComponentRegistry;
use mlua::{Lua, LuaOptions, LuaSerdeExt, StdLib};

use crate::error::ScriptError;
use crate::types::Script;

const NO_RANDOM_MESSAGE: &str =
    "math.random/math.randomseed are disabled: scripts have no RNG access yet (see ADR-0006)";

/// Owns a sandboxed Lua VM and the set of loaded content-script files, and
/// dispatches per-tick calls into `Script`-tagged entities. See ADR-0006 for
/// why this lives outside `engine-core`/`engine-scene` entirely.
pub struct ScriptHost {
    lua: Lua,
    loaded: HashMap<PathBuf, ()>,
}

impl ScriptHost {
    pub fn new() -> Result<Self, ScriptError> {
        let lua = Lua::new_with(StdLib::ALL_SAFE, LuaOptions::default()).map_err(|e| {
            ScriptError::LoadFailed {
                path: "<sandbox init>".to_string(),
                source: e,
            }
        })?;
        disable_ambient_randomness(&lua)?;
        Ok(Self {
            lua,
            loaded: HashMap::new(),
        })
    }

    /// Loads (or reloads) the Lua chunk at `path`, defining its top-level
    /// functions as globals. Idempotent: reloading the same path re-executes
    /// the chunk, replacing any globals it (re)defines.
    pub fn load_file(&mut self, path: &Path) -> Result<(), ScriptError> {
        let path_str = path.display().to_string();
        let src = std::fs::read_to_string(path).map_err(|e| ScriptError::ReadFailed {
            path: path_str.clone(),
            source: e,
        })?;
        self.lua
            .load(&src)
            .set_name(&path_str)
            .exec()
            .map_err(|e| ScriptError::LoadFailed {
                path: path_str.clone(),
                source: e,
            })?;
        self.loaded.insert(path.to_path_buf(), ());
        Ok(())
    }

    /// Distinct script file paths loaded so far, for the CLI's file watcher
    /// to build its watch set from.
    pub fn loaded_paths(&self) -> impl Iterator<Item = &Path> {
        self.loaded.keys().map(PathBuf::as_path)
    }

    /// Calls every `Script`-tagged entity's named function once, in a
    /// deterministic order (sorted by entity id — see ADR-0002: Lua call
    /// order is observable via side effects, unlike native systems today).
    /// Each entity's other registered components are passed in as a table
    /// and any fields the function returns are written back through the
    /// same registry loaders scene files use. Errors are collected per
    /// entity rather than aborting the whole dispatch, so one bad script
    /// doesn't hide problems with its siblings.
    pub fn dispatch(&mut self, mut ctx: DispatchCtx<'_, '_>) -> Vec<(hecs::Entity, ScriptError)> {
        let mut targets: Vec<(hecs::Entity, Script)> = ctx
            .world
            .query::<&Script>()
            .iter()
            .map(|(e, script)| (e, script.clone()))
            .collect();
        targets.sort_by_key(|(e, _)| e.to_bits());

        let mut errors = Vec::new();
        for (entity, script) in targets {
            if let Err(e) = self.dispatch_one(&mut ctx, entity, &script) {
                errors.push((entity, e));
            }
        }
        errors
    }

    fn dispatch_one(
        &mut self,
        ctx: &mut DispatchCtx<'_, '_>,
        entity: hecs::Entity,
        script: &Script,
    ) -> Result<(), ScriptError> {
        let entity_label = format!("{entity:?}");

        let input = {
            let entity_ref = ctx
                .world
                .entity(entity)
                .expect("entity came from this world");
            let mut map = serde_json::Map::new();
            for dumper in ctx.dumpers {
                if let Some((name, value)) = dumper(&entity_ref) {
                    map.insert(name.to_string(), value);
                }
            }
            serde_json::Value::Object(map)
        };

        let func: mlua::Function =
            self.lua
                .globals()
                .get(script.function.as_str())
                .map_err(|_| ScriptError::UnknownFunction {
                    path: script.path.clone(),
                    function: script.function.clone(),
                })?;

        let input_value: mlua::Value =
            self.lua
                .to_value(&input)
                .map_err(|e| ScriptError::RuntimeFailed {
                    path: script.path.clone(),
                    function: script.function.clone(),
                    entity: entity_label.clone(),
                    source: Box::new(e),
                })?;

        let result: mlua::Value =
            func.call((input_value, ctx.tick, ctx.dt))
                .map_err(|e| ScriptError::RuntimeFailed {
                    path: script.path.clone(),
                    function: script.function.clone(),
                    entity: entity_label.clone(),
                    source: Box::new(e),
                })?;

        let output: serde_json::Value =
            self.lua
                .from_value(result)
                .map_err(|e| ScriptError::ResultDecodeFailed {
                    path: script.path.clone(),
                    function: script.function.clone(),
                    entity: entity_label.clone(),
                    source: Box::new(e),
                })?;

        let fields = match output {
            serde_json::Value::Object(map) => map,
            serde_json::Value::Null => return Ok(()),
            other => {
                return Err(ScriptError::ResultDecodeFailed {
                    path: script.path.clone(),
                    function: script.function.clone(),
                    entity: entity_label.clone(),
                    source: Box::new(mlua::Error::RuntimeError(format!(
                        "expected a table of component fields, got {other}"
                    ))),
                })
            }
        };

        let mut builder = hecs::EntityBuilder::new();
        for (name, value) in fields {
            let loader =
                ctx.components
                    .loader(&name)
                    .ok_or_else(|| ScriptError::UnknownComponent {
                        path: script.path.clone(),
                        function: script.function.clone(),
                        entity: entity_label.clone(),
                        component: name.clone(),
                    })?;
            loader(value, &mut builder).map_err(|e| ScriptError::ComponentDeserializeFailed {
                path: script.path.clone(),
                function: script.function.clone(),
                entity: entity_label.clone(),
                component: name.clone(),
                source: e,
            })?;
        }
        ctx.world
            .insert(entity, builder.build())
            .expect("entity came from this world");
        Ok(())
    }
}

/// Bundles what `ScriptHost::dispatch` needs from the caller's `Sim` into
/// one borrow, keeping the method's argument count sane.
pub struct DispatchCtx<'w, 'd> {
    pub world: &'w mut hecs::World,
    pub components: &'d ComponentRegistry,
    pub dumpers: &'d [ComponentDumper],
    pub tick: u64,
    pub dt: f32,
}

fn disable_ambient_randomness(lua: &Lua) -> Result<(), ScriptError> {
    let math: mlua::Table = lua
        .globals()
        .get("math")
        .map_err(|e| ScriptError::LoadFailed {
            path: "<sandbox init>".to_string(),
            source: e,
        })?;
    let deny = lua
        .create_function(|_, _: mlua::Variadic<mlua::Value>| -> mlua::Result<()> {
            Err(mlua::Error::RuntimeError(NO_RANDOM_MESSAGE.to_string()))
        })
        .map_err(|e| ScriptError::LoadFailed {
            path: "<sandbox init>".to_string(),
            source: e,
        })?;
    math.set("random", deny.clone())
        .and_then(|_| math.set("randomseed", deny))
        .map_err(|e| ScriptError::LoadFailed {
            path: "<sandbox init>".to_string(),
            source: e,
        })
}
