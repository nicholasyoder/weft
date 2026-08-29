mod components;
mod error;
mod gpu;
pub mod mesh;

use std::path::Path;

pub use components::{Camera, Material, MeshKind, MeshRef};
pub use error::RenderError;
pub use gpu::render_scene;

/// Renders `world` and writes it to `path` as a PNG. The only entry point
/// `engine-cli` needs — pixel-buffer and `image`-crate details stay inside
/// this crate.
pub fn render_scene_to_png(
    world: &hecs::World,
    width: u32,
    height: u32,
    path: &Path,
) -> Result<(), RenderError> {
    let image = render_scene(world, width, height)?;
    image.save(path).map_err(|e| RenderError::EncodeFailed {
        path: path.display().to_string(),
        source: e,
    })
}
