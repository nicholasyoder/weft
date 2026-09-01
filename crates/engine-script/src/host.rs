use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use engine_core::inspect::ComponentDumper;
use engine_core::rng::EngineRng;
use engine_scene::ComponentRegistry;
use mlua::{Lua, LuaOptions, LuaSerdeExt, StdLib};
use rand::Rng;

use crate::error::ScriptError;
use crate::types::Script;

const NO_RANDOM_MESSAGE: &str =
    "math.random/math.randomseed are disabled: scripts have no RNG access yet (see ADR-0006). Use engine.random/engine.random_int instead (see ADR-0012).";

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
    pub fn dispatch(
        &mut self,
        mut ctx: DispatchCtx<'_, '_, '_>,
    ) -> Vec<(hecs::Entity, ScriptError)> {
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
        ctx: &mut DispatchCtx<'_, '_, '_>,
        entity: hecs::Entity,
        script: &Script,
    ) -> Result<(), ScriptError> {
        let entity_label = format!("{entity:?}");

        let input = {
            let entity_ref = ctx
                .world
                .entity(entity)
                .expect("entity came from this world");
            serde_json::Value::Object(dump_entity(&entity_ref, ctx.dumpers))
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

        // `engine.*` (random/despawn/query, ADR-0012) is bound fresh for
        // this one call via `Lua::scope`: the bound functions close over
        // `ctx.world`/`ctx.rng`, which only live for this `dispatch()` call,
        // not the whole `ScriptHost`. The `Cell`/`RefCell`s let several
        // scoped closures share access — only one is ever live at a time
        // since Lua calls them synchronously, never concurrently.
        let self_despawned = Cell::new(false);
        let world_cell = RefCell::new(&mut *ctx.world);
        let rng_cell = RefCell::new(&mut *ctx.rng);
        let dumpers = ctx.dumpers;
        let self_id = entity.to_bits().get() as f64;
        let lua = &self.lua;

        let result: mlua::Value = lua
            .scope(|scope| {
                let engine_table = lua.create_table()?;

                let random_fn = scope.create_function(|_, (lo, hi): (f64, f64)| {
                    if lo >= hi || lo.is_nan() || hi.is_nan() {
                        return Err(mlua::Error::RuntimeError(format!(
                            "engine.random: lo ({lo}) must be less than hi ({hi})"
                        )));
                    }
                    Ok(rng_cell.borrow_mut().gen_range(lo..hi))
                })?;
                engine_table.set("random", random_fn)?;

                let random_int_fn = scope.create_function(|_, (lo, hi): (i64, i64)| {
                    if lo > hi {
                        return Err(mlua::Error::RuntimeError(format!(
                            "engine.random_int: lo ({lo}) must be <= hi ({hi})"
                        )));
                    }
                    Ok(rng_cell.borrow_mut().gen_range(lo..=hi))
                })?;
                engine_table.set("random_int", random_int_fn)?;

                let despawn_fn = scope.create_function(|_, id: Option<f64>| {
                    let target = match id {
                        None => entity,
                        Some(id) => decode_entity_id(id, "engine.despawn")?,
                    };
                    if target == entity {
                        self_despawned.set(true);
                    }
                    Ok(world_cell.borrow_mut().despawn(target).is_ok())
                })?;
                engine_table.set("despawn", despawn_fn)?;

                let query_fn = scope.create_function(|lua_ctx, names: Vec<String>| {
                    let world = world_cell.borrow();
                    let results = lua_ctx.create_table()?;
                    let mut i = 1i64;
                    for entity_ref in world.iter() {
                        let dumped = dump_entity(&entity_ref, dumpers);
                        if !names.iter().all(|n| dumped.contains_key(n)) {
                            continue;
                        }
                        let row = lua_ctx.create_table()?;
                        row.set("id", entity_ref.entity().to_bits().get() as f64)?;
                        for name in &names {
                            let value = dumped.get(name).expect("checked present above");
                            row.set(name.as_str(), lua_ctx.to_value(value)?)?;
                        }
                        results.set(i, row)?;
                        i += 1;
                    }
                    Ok(results)
                })?;
                engine_table.set("query", query_fn)?;

                lua.globals().set("engine", engine_table)?;
                func.call((input_value, ctx.tick, ctx.dt, self_id))
            })
            .map_err(|e| ScriptError::RuntimeFailed {
                path: script.path.clone(),
                function: script.function.clone(),
                entity: entity_label.clone(),
                source: Box::new(e),
            })?;

        if self_despawned.get() {
            return Ok(());
        }

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

/// Dumps every registered component `dumpers` finds on `entity_ref` into one
/// JSON object, keyed by component name. Shared by the self-input table
/// `dispatch_one` builds and by `engine.query`'s per-entity results (ADR-0012)
/// — both are "everything this entity has, per the same dumper list."
fn dump_entity(
    entity_ref: &hecs::EntityRef,
    dumpers: &[ComponentDumper],
) -> serde_json::Map<String, serde_json::Value> {
    let mut map = serde_json::Map::new();
    for dumper in dumpers {
        if let Some((name, value)) = dumper(entity_ref) {
            map.insert(name.to_string(), value);
        }
    }
    map
}

/// Decodes a Lua-side entity id (an `Entity::to_bits()` `NonZeroU64`, passed
/// through Lua as an f64 — safe up to 2^53, far past any realistic entity
/// count here; see ADR-0012) back into a real `Entity`.
fn decode_entity_id(id: f64, context: &str) -> mlua::Result<hecs::Entity> {
    if id <= 0.0 || id.fract() != 0.0 || id > (1u64 << 53) as f64 {
        return Err(mlua::Error::RuntimeError(format!(
            "{context}: '{id}' is not a valid entity id"
        )));
    }
    hecs::Entity::from_bits(id as u64).ok_or_else(|| {
        mlua::Error::RuntimeError(format!("{context}: '{id}' is not a valid entity id"))
    })
}

/// Bundles what `ScriptHost::dispatch` needs from the caller's `Sim` into
/// one borrow, keeping the method's argument count sane.
pub struct DispatchCtx<'w, 'd, 'r> {
    pub world: &'w mut hecs::World,
    pub components: &'d ComponentRegistry,
    pub dumpers: &'d [ComponentDumper],
    pub rng: &'r mut EngineRng,
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
