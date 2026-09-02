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

/// Screen-space HUD text — no `Transform`, since this is 2D UI, not a 3D
/// billboard (world-space text is a possible future addition, not this
/// one). `(x, y)` is the text block's top-left origin in pixels, `size` its
/// pixel height. `font` is a content hash into `engine-assets` (import any
/// `.ttf`/`.otf` via `engine import`, see ADR-0014); `None` falls back to
/// the engine's embedded default font, so text renders with zero asset
/// authoring required.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Text {
    pub content: String,
    pub x: f32,
    pub y: f32,
    #[serde(default = "default_text_size")]
    pub size: f32,
    #[serde(default = "default_text_color")]
    pub color: [f32; 3],
    #[serde(default)]
    pub font: Option<String>,
}

fn default_text_size() -> f32 {
    24.0
}

fn default_text_color() -> [f32; 3] {
    [1.0, 1.0, 1.0]
}
