pub mod error;
mod format;
pub mod registry;

use std::path::Path;

use engine_core::inspect::ComponentDumper;
use engine_core::sim::Sim;

pub use error::SceneError;
pub use registry::{ComponentLoader, ComponentRegistry, SystemRegistry};

/// Every entity spawned from a scene file carries this so `engine inspect`
/// output can be diffed by the name the file author chose, instead of by
/// hecs's internal (and spawn-order-derived) entity id.
struct SceneName(String);

fn dump_scene_name(e: &hecs::EntityRef) -> Option<(&'static str, serde_json::Value)> {
    e.get::<&SceneName>()
        .map(|n| ("SceneName", serde_json::Value::String(n.0.clone())))
}

const DEFAULT_DT: f32 = 1.0 / 60.0;

/// Parses the scene file at `path`, spawns its entities (in file order) and
/// attaches its systems into a fresh `Sim` seeded with `seed`, and returns
/// the dumper list `engine_core::inspect::world_to_json` needs to render it.
pub fn load(
    path: &Path,
    seed: u64,
    components: &ComponentRegistry,
    systems: &SystemRegistry,
) -> Result<(Sim, Vec<ComponentDumper>), SceneError> {
    let path_str = path.display().to_string();
    let text = std::fs::read_to_string(path).map_err(|e| SceneError::ReadFailed {
        path: path_str.clone(),
        source: e,
    })?;
    let scene: format::SceneDef = toml::from_str(&text).map_err(|e| SceneError::ParseFailed {
        path: path_str.clone(),
        source: e,
    })?;

    let mut sim = Sim::new(seed, scene.meta.dt.unwrap_or(DEFAULT_DT));

    for entity in &scene.entities {
        let mut builder = hecs::EntityBuilder::new();
        builder.add(SceneName(entity.name.clone()));
        for (component_name, value) in &entity.components {
            let loader =
                components
                    .loader(component_name)
                    .ok_or_else(|| SceneError::UnknownComponent {
                        path: path_str.clone(),
                        entity: entity.name.clone(),
                        component: component_name.clone(),
                    })?;
            let json = serde_json::to_value(value).expect("toml::Value always serializes");
            loader(json, &mut builder).map_err(|e| SceneError::ComponentDeserializeFailed {
                path: path_str.clone(),
                entity: entity.name.clone(),
                component: component_name.clone(),
                source: e,
            })?;
        }
        sim.world.spawn(builder.build());
    }

    for system in &scene.systems {
        let f = systems
            .find(&system.name)
            .ok_or_else(|| SceneError::UnknownSystem {
                path: path_str.clone(),
                system: system.name.clone(),
            })?;
        sim.scheduler_mut().add_system(system.name.clone(), f);
    }

    let mut dumpers = vec![dump_scene_name as ComponentDumper];
    dumpers.extend(components.dumpers());
    Ok((sim, dumpers))
}
