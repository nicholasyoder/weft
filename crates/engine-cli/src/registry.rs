//! The one `ComponentRegistry`/`SystemRegistry` engine-cli knows about,
//! wiring scene-file component/system names to the same Rust types the
//! hardcoded `basic` scenario and the renderer already use. There is no
//! separate game crate yet, so this is the single place scene files,
//! hardcoded scenarios, and rendering share component definitions.

use engine_anim::{animation_step, Animator, JointPalette};
use engine_audio::{audio_step, AudioSource, SoundsPlayed};
use engine_core::Transform;
use engine_physics::{physics_step, Collider, RigidBody};
use engine_render::{Camera, Light, Material, MeshRef, Text};
use engine_scene::{ComponentRegistry, SystemRegistry};
use engine_script::Script;

use crate::scenarios::basic::{movement_system, Position, Velocity};
use crate::scenarios::despawn_demo::{despawn_after_system, DespawnAfter};
use crate::scenarios::scripted_demo::Fuse;

/// Supplies a component type's scene-file name, so [`load`]/[`dump`] can be
/// generic over `T` instead of every component needing its own hand-written
/// `load_x`/`dump_x` pair (see `docs/roadmap/known-issues.md`'s
/// registry-boilerplate item).
pub trait Named {
    const NAME: &'static str;
}

/// Deserializes any `Named` component from its scene-file JSON value.
/// Registered per type as `load::<T>` — an ordinary monomorphized function
/// item, which coerces to the plain `fn` pointer `ComponentLoader` expects
/// exactly like a hand-written loader would.
pub fn load<T>(v: serde_json::Value, b: &mut hecs::EntityBuilder) -> Result<(), serde_json::Error>
where
    T: serde::de::DeserializeOwned + hecs::Component,
{
    b.add(serde_json::from_value::<T>(v)?);
    Ok(())
}

/// Dumps any `Named` component to its scene-file JSON value. Bound on
/// `Serialize` alone (not `Clone`) — `serde_json::to_value(&*c)` uses
/// serde's blanket `impl Serialize for &T`, so this covers `Copy` types,
/// `Clone`-only types, and types with neither (e.g. `Position`/`Velocity`)
/// uniformly.
pub fn dump<T>(e: &hecs::EntityRef) -> Option<(&'static str, serde_json::Value)>
where
    T: Named + serde::Serialize + hecs::Component,
{
    e.get::<&T>()
        .map(|c| (T::NAME, serde_json::to_value(&*c).unwrap()))
}

impl Named for Transform {
    const NAME: &'static str = "Transform";
}
impl Named for Camera {
    const NAME: &'static str = "Camera";
}
impl Named for MeshRef {
    const NAME: &'static str = "MeshRef";
}
impl Named for Material {
    const NAME: &'static str = "Material";
}
impl Named for Text {
    const NAME: &'static str = "Text";
}
impl Named for Light {
    const NAME: &'static str = "Light";
}
impl Named for Script {
    const NAME: &'static str = "Script";
}
impl Named for RigidBody {
    const NAME: &'static str = "RigidBody";
}
impl Named for Collider {
    const NAME: &'static str = "Collider";
}
/// `JointPalette` is computed by `animation_step` every tick, never
/// hand-authored — but it's registered with a real loader (not a rejecting
/// stub) rather than inventing a "dump-only" registry mechanism, since a
/// scene-authored initial value is harmless and gets overwritten the very
/// next tick an `Animator` is present, the same relationship `Transform`
/// already has with `physics_step`.
impl Named for Animator {
    const NAME: &'static str = "Animator";
}
impl Named for JointPalette {
    const NAME: &'static str = "JointPalette";
}
impl Named for AudioSource {
    const NAME: &'static str = "AudioSource";
}
/// `SoundsPlayed` is computed by `audio_step` every tick, never
/// hand-authored — registered with a real loader anyway, same "harmless
/// scene-authored initial value, overwritten next tick" posture
/// `JointPalette` established (ADR-0015).
impl Named for SoundsPlayed {
    const NAME: &'static str = "SoundsPlayed";
}

pub fn components() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    registry.register("Position", load::<Position>, dump::<Position>);
    registry.register("Velocity", load::<Velocity>, dump::<Velocity>);
    registry.register("Transform", load::<Transform>, dump::<Transform>);
    registry.register("Camera", load::<Camera>, dump::<Camera>);
    registry.register("MeshRef", load::<MeshRef>, dump::<MeshRef>);
    registry.register("Material", load::<Material>, dump::<Material>);
    registry.register("Text", load::<Text>, dump::<Text>);
    registry.register("Light", load::<Light>, dump::<Light>);
    registry.register("Script", load::<Script>, dump::<Script>);
    registry.register("RigidBody", load::<RigidBody>, dump::<RigidBody>);
    registry.register("Collider", load::<Collider>, dump::<Collider>);
    registry.register("DespawnAfter", load::<DespawnAfter>, dump::<DespawnAfter>);
    registry.register("Fuse", load::<Fuse>, dump::<Fuse>);
    registry.register("Animator", load::<Animator>, dump::<Animator>);
    registry.register("JointPalette", load::<JointPalette>, dump::<JointPalette>);
    registry.register("AudioSource", load::<AudioSource>, dump::<AudioSource>);
    registry.register("SoundsPlayed", load::<SoundsPlayed>, dump::<SoundsPlayed>);
    registry
}

pub fn systems() -> SystemRegistry {
    let mut registry = SystemRegistry::new();
    registry.register("movement", movement_system);
    registry.register("physics", physics_step);
    registry.register("despawn-after", despawn_after_system);
    registry.register("animation", animation_step);
    registry.register("audio", audio_step);
    registry
}
