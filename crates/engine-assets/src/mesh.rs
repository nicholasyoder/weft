use serde::{Deserialize, Serialize};

use crate::error::AssetError;

/// Plain mesh data, deliberately free of any wgpu/bytemuck types — this
/// crate stays renderer-agnostic; `engine-render` depends on it, not the
/// reverse.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeshData {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

/// `bincode`'s default config is deterministic for plain `Vec`/numeric data
/// (no maps), so encoding the same `MeshData` twice always yields the same
/// bytes — the property the content-addressed store depends on.
pub fn encode(data: &MeshData) -> Result<Vec<u8>, AssetError> {
    bincode::serialize(data).map_err(|e| AssetError::MeshEncodeFailed(e.to_string()))
}

pub fn decode(bytes: &[u8]) -> Result<MeshData, AssetError> {
    bincode::deserialize(bytes).map_err(|e| AssetError::MeshDecodeFailed(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_is_deterministic() {
        let data = MeshData {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]],
            normals: vec![[0.0, 1.0, 0.0], [0.0, 1.0, 0.0]],
            uvs: vec![[0.0, 0.0], [1.0, 0.0]],
            indices: vec![0, 1],
        };
        let a = encode(&data).unwrap();
        let b = encode(&data).unwrap();
        assert_eq!(a, b);
        assert_eq!(decode(&a).unwrap(), data);
    }
}
