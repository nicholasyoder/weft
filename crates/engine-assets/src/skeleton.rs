use serde::{Deserialize, Serialize};

use crate::error::AssetError;

/// A local translation/rotation(quaternion xyzw)/scale transform, plain
/// arrays rather than `glam` types — same "renderer-agnostic plain data"
/// convention as `mesh::MeshData`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Trs {
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Joint {
    /// Index into `Skeleton.joints`, always `< self`'s own index when
    /// `Some` — see `Skeleton`'s own doc comment.
    pub parent: Option<u16>,
    pub inverse_bind_matrix: [[f32; 4]; 4],
    pub local_rest_transform: Trs,
}

/// A joint hierarchy, stored **root-first**: for every joint `i`,
/// `joints[i].parent` (if any) is `< i`. This is what lets sampling compose
/// world transforms in a single forward pass with no sorting or recursion
/// needed at sample time. Kept as its own asset type, separate from
/// `AnimationClip`, so Tier 3's multi-clip/blending work (multiple clips
/// driving one skeleton) doesn't need a second breaking asset-format
/// migration — see ADR-0015.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Skeleton {
    pub joints: Vec<Joint>,
}

pub fn encode(data: &Skeleton) -> Result<Vec<u8>, AssetError> {
    bincode::serialize(data).map_err(|e| AssetError::SkeletonEncodeFailed(e.to_string()))
}

pub fn decode(bytes: &[u8]) -> Result<Skeleton, AssetError> {
    bincode::deserialize(bytes).map_err(|e| AssetError::SkeletonDecodeFailed(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity_trs() -> Trs {
        Trs {
            translation: [0.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        }
    }

    #[test]
    fn round_trips_and_is_deterministic() {
        let data = Skeleton {
            joints: vec![
                Joint {
                    parent: None,
                    inverse_bind_matrix: [[1.0; 4]; 4],
                    local_rest_transform: identity_trs(),
                },
                Joint {
                    parent: Some(0),
                    inverse_bind_matrix: [[2.0; 4]; 4],
                    local_rest_transform: identity_trs(),
                },
            ],
        };
        let a = encode(&data).unwrap();
        let b = encode(&data).unwrap();
        assert_eq!(a, b);
        assert_eq!(decode(&a).unwrap(), data);
    }
}
