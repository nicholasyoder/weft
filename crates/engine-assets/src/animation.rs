use serde::{Deserialize, Serialize};

use crate::error::AssetError;

/// A sparse keyframe track: `times[i]` (seconds, strictly increasing) maps
/// to `values[i]`. Sampling holds the boundary value outside `[times[0],
/// times[last]]` and interpolates between the surrounding pair otherwise —
/// see `engine-anim`'s `sampling` module, the only consumer of this shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Keyframes<T> {
    pub times: Vec<f32>,
    pub values: Vec<T>,
}

/// One joint's animated channels within a clip. Any of the three may be
/// absent (e.g. a joint that only rotates has no `translation`/`scale`
/// track) — an absent track means that component holds the joint's
/// `Skeleton`-authored rest value for the whole clip.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JointTrack {
    /// Index into the `Skeleton` this clip is meant to be sampled against.
    pub joint: u16,
    pub translation: Option<Keyframes<[f32; 3]>>,
    pub rotation: Option<Keyframes<[f32; 4]>>,
    pub scale: Option<Keyframes<[f32; 3]>>,
}

/// A single animation clip: a fixed `duration` (seconds) and per-joint
/// keyframe tracks. Deliberately one clip per asset, not a clip *set* —
/// see ADR-0015 on why `Skeleton`/`AnimationClip` are split rather than
/// combined, ahead of Tier 3's multi-clip work.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimationClip {
    pub duration: f32,
    pub tracks: Vec<JointTrack>,
}

pub fn encode(data: &AnimationClip) -> Result<Vec<u8>, AssetError> {
    bincode::serialize(data).map_err(|e| AssetError::AnimationEncodeFailed(e.to_string()))
}

pub fn decode(bytes: &[u8]) -> Result<AnimationClip, AssetError> {
    bincode::deserialize(bytes).map_err(|e| AssetError::AnimationDecodeFailed(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_is_deterministic() {
        let data = AnimationClip {
            duration: 1.0,
            tracks: vec![JointTrack {
                joint: 1,
                translation: None,
                rotation: Some(Keyframes {
                    times: vec![0.0, 0.5, 1.0],
                    values: vec![
                        [0.0, 0.0, 0.0, 1.0],
                        [0.0, 0.0, 0.38, 0.92],
                        [0.0, 0.0, 0.0, 1.0],
                    ],
                }),
                scale: None,
            }],
        };
        let a = encode(&data).unwrap();
        let b = encode(&data).unwrap();
        assert_eq!(a, b);
        assert_eq!(decode(&a).unwrap(), data);
    }
}
