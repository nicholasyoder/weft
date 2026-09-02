use std::path::Path;

use crate::error::AssetError;
use crate::store::{self, AssetStore};

/// Content-addresses an audio file's raw bytes verbatim — no parsing at
/// import time. Mirrors `import_font`'s bytes-in-hash-out shape exactly:
/// `engine-assets` doesn't understand audio container/codec formats any
/// more than it understands font formats — `engine-audio` decodes lazily,
/// at clip-cache time (see ADR-0016).
pub fn import_audio(path: &Path, store: &AssetStore) -> Result<String, AssetError> {
    let bytes = store::read_file(path)?;
    store.put(&bytes)
}
