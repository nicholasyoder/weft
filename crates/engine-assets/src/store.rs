use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::error::AssetError;

/// A content-addressed blob store, laid out like git's own object store
/// (`root/<hash[0:2]>/<hash>`, no file extensions — the component that
/// references a hash already knows how to interpret it). Identical content
/// always lands at the same path, which is the whole mechanism behind
/// re-importing an unchanged file producing zero new/changed files.
pub struct AssetStore {
    root: PathBuf,
}

impl AssetStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn path_for(&self, hash: &str) -> PathBuf {
        self.root.join(&hash[0..2]).join(hash)
    }

    /// Writes `bytes` under their sha256 digest, returning the hex digest.
    /// A no-op if the content is already present.
    pub fn put(&self, bytes: &[u8]) -> Result<String, AssetError> {
        let hash = hex_digest(bytes);
        let path = self.path_for(&hash);
        if !path.exists() {
            let dir = path.parent().expect("path_for always has a parent");
            std::fs::create_dir_all(dir).map_err(|source| AssetError::Io {
                path: dir.display().to_string(),
                source,
            })?;
            std::fs::write(&path, bytes).map_err(|source| AssetError::Io {
                path: path.display().to_string(),
                source,
            })?;
        }
        Ok(hash)
    }

    pub fn get(&self, hash: &str) -> Result<Vec<u8>, AssetError> {
        let path = self.path_for(hash);
        std::fs::read(&path).map_err(|source| {
            if source.kind() == std::io::ErrorKind::NotFound {
                AssetError::NotFound(hash.to_string())
            } else {
                AssetError::Io {
                    path: path.display().to_string(),
                    source,
                }
            }
        })
    }
}

fn hex_digest(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

pub(crate) fn read_file(path: &Path) -> Result<Vec<u8>, AssetError> {
    std::fs::read(path).map_err(|source| AssetError::Io {
        path: path.display().to_string(),
        source,
    })
}
