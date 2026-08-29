use std::path::Path;

use crate::error::AssetError;
use crate::store::{self, AssetStore};

/// Decodes an image file (any format the `image` crate supports) and stores
/// it re-encoded as PNG, so the render-side loader only ever needs one
/// decode path regardless of the source format.
pub fn import_texture(path: &Path, store: &AssetStore) -> Result<String, AssetError> {
    let bytes = store::read_file(path)?;
    let hash = import_texture_bytes(&bytes, &path.display().to_string(), store)?;
    Ok(hash)
}

pub(crate) fn import_texture_bytes(
    bytes: &[u8],
    label: &str,
    store: &AssetStore,
) -> Result<String, AssetError> {
    let decoded =
        image::load_from_memory(bytes).map_err(|source| AssetError::ImageDecodeFailed {
            path: label.to_string(),
            source,
        })?;
    let mut png_bytes = Vec::new();
    decoded
        .to_rgba8()
        .write_to(
            &mut std::io::Cursor::new(&mut png_bytes),
            image::ImageFormat::Png,
        )
        .map_err(|source| AssetError::ImageDecodeFailed {
            path: label.to_string(),
            source,
        })?;
    store.put(&png_bytes)
}
