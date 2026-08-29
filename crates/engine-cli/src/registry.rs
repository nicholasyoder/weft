//! The one `ComponentRegistry`/`SystemRegistry` engine-cli knows about,
//! wiring scene-file component/system names to the same Rust types the
//! hardcoded `basic` scenario already uses. There is no separate game crate
//! yet, so this is the single place scene files and hardcoded scenarios
//! share component definitions.

use engine_scene::{ComponentRegistry, SystemRegistry};

use crate::scenarios::basic::{dump_position, dump_velocity, movement_system, Position, Velocity};

fn load_position(
    v: serde_json::Value,
    b: &mut hecs::EntityBuilder,
) -> Result<(), serde_json::Error> {
    b.add(serde_json::from_value::<Position>(v)?);
    Ok(())
}

fn load_velocity(
    v: serde_json::Value,
    b: &mut hecs::EntityBuilder,
) -> Result<(), serde_json::Error> {
    b.add(serde_json::from_value::<Velocity>(v)?);
    Ok(())
}

pub fn components() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    registry.register("Position", load_position, dump_position);
    registry.register("Velocity", load_velocity, dump_velocity);
    registry
}

pub fn systems() -> SystemRegistry {
    let mut registry = SystemRegistry::new();
    registry.register("movement", movement_system);
    registry
}
