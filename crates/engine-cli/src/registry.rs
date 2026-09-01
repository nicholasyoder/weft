//! The one `ComponentRegistry`/`SystemRegistry` engine-cli knows about,
//! wiring scene-file component/system names to the same Rust types the
//! hardcoded `basic` scenario and the renderer already use. There is no
//! separate game crate yet, so this is the single place scene files,
//! hardcoded scenarios, and rendering share component definitions.

use engine_core::Transform;
use engine_physics::{physics_step, Collider, RigidBody};
use engine_render::{Camera, Material, MeshRef};
use engine_scene::{ComponentRegistry, SystemRegistry};
use engine_script::Script;

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

fn load_transform(
    v: serde_json::Value,
    b: &mut hecs::EntityBuilder,
) -> Result<(), serde_json::Error> {
    b.add(serde_json::from_value::<Transform>(v)?);
    Ok(())
}

pub(crate) fn dump_transform(e: &hecs::EntityRef) -> Option<(&'static str, serde_json::Value)> {
    e.get::<&Transform>()
        .map(|t| ("Transform", serde_json::to_value(*t).unwrap()))
}

fn load_camera(v: serde_json::Value, b: &mut hecs::EntityBuilder) -> Result<(), serde_json::Error> {
    b.add(serde_json::from_value::<Camera>(v)?);
    Ok(())
}

fn dump_camera(e: &hecs::EntityRef) -> Option<(&'static str, serde_json::Value)> {
    e.get::<&Camera>()
        .map(|c| ("Camera", serde_json::to_value(*c).unwrap()))
}

fn load_mesh_ref(
    v: serde_json::Value,
    b: &mut hecs::EntityBuilder,
) -> Result<(), serde_json::Error> {
    b.add(serde_json::from_value::<MeshRef>(v)?);
    Ok(())
}

fn dump_mesh_ref(e: &hecs::EntityRef) -> Option<(&'static str, serde_json::Value)> {
    e.get::<&MeshRef>()
        .map(|m| ("MeshRef", serde_json::to_value((*m).clone()).unwrap()))
}

fn load_material(
    v: serde_json::Value,
    b: &mut hecs::EntityBuilder,
) -> Result<(), serde_json::Error> {
    b.add(serde_json::from_value::<Material>(v)?);
    Ok(())
}

fn dump_material(e: &hecs::EntityRef) -> Option<(&'static str, serde_json::Value)> {
    e.get::<&Material>()
        .map(|m| ("Material", serde_json::to_value((*m).clone()).unwrap()))
}

fn load_script(v: serde_json::Value, b: &mut hecs::EntityBuilder) -> Result<(), serde_json::Error> {
    b.add(serde_json::from_value::<Script>(v)?);
    Ok(())
}

fn dump_script(e: &hecs::EntityRef) -> Option<(&'static str, serde_json::Value)> {
    e.get::<&Script>()
        .map(|s| ("Script", serde_json::to_value((*s).clone()).unwrap()))
}

fn load_rigid_body(
    v: serde_json::Value,
    b: &mut hecs::EntityBuilder,
) -> Result<(), serde_json::Error> {
    b.add(serde_json::from_value::<RigidBody>(v)?);
    Ok(())
}

pub(crate) fn dump_rigid_body(e: &hecs::EntityRef) -> Option<(&'static str, serde_json::Value)> {
    e.get::<&RigidBody>()
        .map(|r| ("RigidBody", serde_json::to_value(*r).unwrap()))
}

fn load_collider(
    v: serde_json::Value,
    b: &mut hecs::EntityBuilder,
) -> Result<(), serde_json::Error> {
    b.add(serde_json::from_value::<Collider>(v)?);
    Ok(())
}

pub(crate) fn dump_collider(e: &hecs::EntityRef) -> Option<(&'static str, serde_json::Value)> {
    e.get::<&Collider>()
        .map(|c| ("Collider", serde_json::to_value(*c).unwrap()))
}

pub fn components() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    registry.register("Position", load_position, dump_position);
    registry.register("Velocity", load_velocity, dump_velocity);
    registry.register("Transform", load_transform, dump_transform);
    registry.register("Camera", load_camera, dump_camera);
    registry.register("MeshRef", load_mesh_ref, dump_mesh_ref);
    registry.register("Material", load_material, dump_material);
    registry.register("Script", load_script, dump_script);
    registry.register("RigidBody", load_rigid_body, dump_rigid_body);
    registry.register("Collider", load_collider, dump_collider);
    registry
}

pub fn systems() -> SystemRegistry {
    let mut registry = SystemRegistry::new();
    registry.register("movement", movement_system);
    registry.register("physics", physics_step);
    registry
}
