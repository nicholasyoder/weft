use serde::{Deserialize, Serialize};

use crate::error::AssetError;

/// Per-vertex skinning data, vertex-index-aligned to a `MeshData`'s
/// `positions` — same convention `normals`/`uvs` already use. Stored as a
/// separate content-addressed asset (not folded into `MeshData`) so
/// unskinned meshes' encoding, and therefore their content hash, never
/// changes (see ADR-0015). At most 4 joint influences per vertex, matching
/// glTF's default `JOINTS_0`/`WEIGHTS_0` accessors.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkinData {
    pub joints: Vec<[u16; 4]>,
    pub weights: Vec<[f32; 4]>,
}

/// `bincode`'s default config is deterministic for this shape, the same
/// property `mesh::encode` relies on for content-addressing.
pub fn encode(data: &SkinData) -> Result<Vec<u8>, AssetError> {
    bincode::serialize(data).map_err(|e| AssetError::SkinEncodeFailed(e.to_string()))
}

pub fn decode(bytes: &[u8]) -> Result<SkinData, AssetError> {
    bincode::deserialize(bytes).map_err(|e| AssetError::SkinDecodeFailed(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_is_deterministic() {
        let data = SkinData {
            joints: vec![[0, 1, 0, 0], [1, 2, 0, 0]],
            weights: vec![[0.6, 0.4, 0.0, 0.0], [0.3, 0.7, 0.0, 0.0]],
        };
        let a = encode(&data).unwrap();
        let b = encode(&data).unwrap();
        assert_eq!(a, b);
        assert_eq!(decode(&a).unwrap(), data);
    }
}
