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
    /// Content hash of an `engine_assets::skin::SkinData` asset (see
    /// ADR-0015) — present only for a skinned mesh, joined with `mesh`'s
    /// own vertex data at draw time to build the GPU-skinned vertex
    /// buffer. `None` for an ordinary (non-skinned) mesh, the overwhelming
    /// majority of `MeshRef`s, so this is `#[serde(default)]` to keep every
    /// existing scene file's `MeshRef` table unchanged.
    #[serde(default)]
    pub skin: Option<String>,
}

/// Base color (optionally tinting a base color texture, referenced by
/// content hash, sampled per-pixel), shaded via a metallic-roughness PBR
/// BRDF (see `visual-realism-plan.md` Phase 1 / ADR-0019). `roughness`
/// defaults to `1.0` (fully rough) and `metallic` to `0.0` (fully
/// dielectric) so an unmodified pre-PBR scene file keeps looking as close
/// to its old flat-Lambertian appearance as the BRDF change allows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Material {
    pub color: [f32; 3],
    #[serde(default)]
    pub texture: Option<String>,
    #[serde(default = "default_roughness")]
    pub roughness: f32,
    #[serde(default)]
    pub metallic: f32,
}

fn default_roughness() -> f32 {
    1.0
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A pre-PBR scene file's `Material` table has only `color`/`texture`
    /// keys — `roughness`/`metallic` must still deserialize to their
    /// documented fully-rough/fully-dielectric defaults, not fail to parse.
    #[test]
    fn material_without_roughness_or_metallic_fields_gets_pbr_defaults() {
        let json = serde_json::json!({ "color": [0.8, 0.2, 0.2] });
        let material: Material = serde_json::from_value(json).unwrap();
        assert_eq!(material.color, [0.8, 0.2, 0.2]);
        assert_eq!(material.texture, None);
        assert_eq!(material.roughness, 1.0);
        assert_eq!(material.metallic, 0.0);
    }
}
