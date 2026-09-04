use serde::{Deserialize, Serialize};

use crate::error::AssetError;

/// Per-vertex tangent data, vertex-index-aligned to a `MeshData`'s
/// `positions` — same convention `SkinData` already uses. Stored as a
/// separate content-addressed asset (not folded into `MeshData`) so meshes
/// without a normal map never pay for it and their content hash never
/// changes (see ADR-0015's precedent, `visual-realism-plan.md` Phase 3).
/// `xyz` is the tangent direction, `w` its handedness sign (`+1.0`/`-1.0`)
/// used to derive the bitangent as `cross(normal, tangent) * w`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TangentData {
    pub tangents: Vec<[f32; 4]>,
}

/// `bincode`'s default config is deterministic for this shape, the same
/// property `mesh::encode` relies on for content-addressing.
pub fn encode(data: &TangentData) -> Result<Vec<u8>, AssetError> {
    bincode::serialize(data).map_err(|e| AssetError::TangentEncodeFailed(e.to_string()))
}

pub fn decode(bytes: &[u8]) -> Result<TangentData, AssetError> {
    bincode::deserialize(bytes).map_err(|e| AssetError::TangentDecodeFailed(e.to_string()))
}

/// Generates per-vertex tangents for a triangle mesh via the standard
/// Lengyel per-triangle accumulation: each triangle's tangent/bitangent
/// (derived from its edge vectors and UV deltas) is accumulated onto its 3
/// vertices, then each vertex's accumulated tangent is Gram-Schmidt
/// orthogonalized against its normal, with the handedness sign recovered
/// from the accumulated bitangent.
///
/// Never panics or produces NaN: a triangle with (near-)zero UV area
/// contributes nothing (rather than dividing by ~zero), and a vertex whose
/// accumulated tangent is (near-)zero (isolated, or every incident
/// triangle was degenerate) falls back to an arbitrary unit vector
/// orthogonal to its normal instead of normalizing a zero vector.
pub fn generate(
    positions: &[[f32; 3]],
    normals: &[[f32; 3]],
    uvs: &[[f32; 2]],
    indices: &[u32],
) -> Vec<[f32; 4]> {
    use glam::Vec3;

    const EPSILON: f32 = 1e-8;

    let mut tan_accum = vec![Vec3::ZERO; positions.len()];
    let mut bitan_accum = vec![Vec3::ZERO; positions.len()];

    for tri in indices.as_chunks::<3>().0 {
        let (i0, i1, i2) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
        let p0 = Vec3::from(positions[i0]);
        let p1 = Vec3::from(positions[i1]);
        let p2 = Vec3::from(positions[i2]);
        let [u0, v0] = uvs[i0];
        let [u1, v1] = uvs[i1];
        let [u2, v2] = uvs[i2];

        let e1 = p1 - p0;
        let e2 = p2 - p0;
        let (du1, dv1) = (u1 - u0, v1 - v0);
        let (du2, dv2) = (u2 - u0, v2 - v0);

        let f = du1 * dv2 - du2 * dv1;
        if f.abs() < EPSILON {
            continue; // degenerate UV area — no contribution from this triangle
        }
        let r = 1.0 / f;
        let tangent = (e1 * dv2 - e2 * dv1) * r;
        let bitangent = (e2 * du1 - e1 * du2) * r;

        for i in [i0, i1, i2] {
            tan_accum[i] += tangent;
            bitan_accum[i] += bitangent;
        }
    }

    (0..positions.len())
        .map(|i| {
            let n = Vec3::from(normals[i]);
            let t = tan_accum[i];
            let orthogonal = t - n * n.dot(t);
            let tangent = if orthogonal.length_squared() < EPSILON {
                arbitrary_orthogonal(n)
            } else {
                orthogonal.normalize()
            };
            let handedness = if n.cross(tangent).dot(bitan_accum[i]) < 0.0 {
                -1.0
            } else {
                1.0
            };
            [tangent.x, tangent.y, tangent.z, handedness]
        })
        .collect()
}

/// An arbitrary unit vector orthogonal to `n` — used internally by
/// `generate` when a vertex's accumulated tangent degenerates to
/// (near-)zero, and exposed for the same reason a caller with no tangent
/// data at all needs a per-vertex fallback (see `engine_render::mesh`'s
/// `from_asset`/`from_skinned_asset`): a single hardcoded fallback
/// direction like `(1,0,0)` can end up exactly parallel to a real vertex
/// normal (an axis-aligned box face, say), degenerating the shader's own
/// Gram-Schmidt step to a zero-length/NaN tangent — deriving the fallback
/// from the actual normal avoids that regardless of mesh orientation.
pub fn arbitrary_orthogonal(n: glam::Vec3) -> glam::Vec3 {
    let helper = if n.x.abs() < 0.9 {
        glam::Vec3::X
    } else {
        glam::Vec3::Y
    };
    helper.cross(n).normalize()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_is_deterministic() {
        let data = TangentData {
            tangents: vec![[1.0, 0.0, 0.0, 1.0], [0.0, 0.0, 1.0, -1.0]],
        };
        let a = encode(&data).unwrap();
        let b = encode(&data).unwrap();
        assert_eq!(a, b);
        assert_eq!(decode(&a).unwrap(), data);
    }

    /// A single triangle in the XY plane, normal +Z, UVs increasing along
    /// +X and +Y — the tangent (which follows increasing U) must point
    /// along +X.
    #[test]
    fn generate_produces_the_expected_tangent_for_a_simple_triangle() {
        let positions = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let normals = [[0.0, 0.0, 1.0]; 3];
        let uvs = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
        let indices = [0u32, 1, 2];

        let tangents = generate(&positions, &normals, &uvs, &indices);
        assert_eq!(tangents.len(), 3);
        for t in tangents {
            assert!(
                (t[0] - 1.0).abs() < 1e-5,
                "expected tangent along +X: {t:?}"
            );
            assert!(t[1].abs() < 1e-5);
            assert!(t[2].abs() < 1e-5);
        }
    }

    /// A triangle whose 3 UVs are identical (zero UV area) must not panic
    /// or produce NaN — every vertex falls back to `arbitrary_orthogonal`.
    #[test]
    fn generate_handles_a_degenerate_zero_uv_area_triangle_without_nan_or_panic() {
        let positions = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let normals = [[0.0, 0.0, 1.0]; 3];
        let uvs = [[0.5, 0.5]; 3];
        let indices = [0u32, 1, 2];

        let tangents = generate(&positions, &normals, &uvs, &indices);
        assert_eq!(tangents.len(), 3);
        for t in tangents {
            for c in t {
                assert!(c.is_finite(), "expected finite tangent component: {t:?}");
            }
            let len_sq = t[0] * t[0] + t[1] * t[1] + t[2] * t[2];
            assert!((len_sq - 1.0).abs() < 1e-4, "expected unit tangent: {t:?}");
        }
    }
}
