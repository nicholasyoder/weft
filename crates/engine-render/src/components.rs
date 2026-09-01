use glam::Vec3;
use serde::{Deserialize, Serialize};

/// A camera looks from its entity's `Transform.position` toward `target`.
/// Using an explicit look-at point rather than `Transform.rotation` is far
/// easier to hand-author in a scene text file than a rotation quaternion —
/// the camera entity's `Transform.rotation` is simply unused.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Camera {
    pub target: Vec3,
    #[serde(default = "default_fov_y_degrees")]
    pub fov_y_degrees: f32,
    #[serde(default = "default_near")]
    pub near: f32,
    #[serde(default = "default_far")]
    pub far: f32,
}

fn default_fov_y_degrees() -> f32 {
    45.0
}

fn default_near() -> f32 {
    0.1
}

fn default_far() -> f32 {
    100.0
}

/// Hardcoded cube/plane geometry, or an imported mesh referenced by its
/// content hash in the `engine-assets` store (see Phase 3 / ADR-0005).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MeshKind {
    Cube,
    Plane,
    Sphere,
    Asset(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeshRef {
    pub mesh: MeshKind,
}

/// Flat base color, modulated by a single hardcoded directional light in the
/// fragment shader, and optionally tinting a base color texture (referenced
/// by content hash) sampled per-pixel. Full PBR is not a goal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Material {
    pub color: [f32; 3],
    #[serde(default)]
    pub texture: Option<String>,
}
