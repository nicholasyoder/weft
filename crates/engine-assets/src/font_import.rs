use std::path::Path;

use crate::error::AssetError;
use crate::store::{self, AssetStore};

/// Content-addresses a font file's raw bytes verbatim — no parsing at
/// import time. Mirrors `import_texture`'s bytes-in-hash-out shape, minus
/// the decode/re-encode step: there's no normalization to do for a font the
/// way there is for an image format, and `engine-assets` deliberately
/// doesn't understand font formats — `engine-render` does that lazily, at
/// draw time (see ADR-0014).
pub fn import_font(path: &Path, store: &AssetStore) -> Result<String, AssetError> {
    let bytes = store::read_file(path)?;
    store.put(&bytes)
}
