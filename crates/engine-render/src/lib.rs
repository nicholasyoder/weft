mod components;
mod error;
mod gpu;
pub mod mesh;
mod text;
mod window;

use std::path::Path;

pub use components::{Camera, Light, LightKind, Material, MeshKind, MeshRef, Text};
pub use error::RenderError;
pub use gpu::{render_scene, render_scene_with_context, RenderContext};
pub use window::WindowRenderer;

/// Renders `world` and writes it to `path` as a PNG. The only entry point
/// `engine-cli` needs — pixel-buffer and `image`-crate details stay inside
/// this crate. `assets_dir` is where `MeshKind::Asset`/textured `Material`
/// content hashes are resolved from.
pub fn render_scene_to_png(
    world: &hecs::World,
    width: u32,
    height: u32,
    assets_dir: &Path,
    path: &Path,
) -> Result<(), RenderError> {
    let image = render_scene(world, width, height, assets_dir)?;
    image.save(path).map_err(|e| RenderError::EncodeFailed {
        path: path.display().to_string(),
        source: e,
    })
}
