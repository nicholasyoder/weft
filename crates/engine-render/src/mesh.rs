use glam::Vec3;

use crate::error::RenderError;

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    /// `xyz` tangent direction, `w` handedness sign (`+1.0`/`-1.0`) used to
    /// derive the bitangent as `cross(normal, tangent) * w` — see Phase 3 /
    /// ADR-0019. Unconditional (not `Option`) so there's one vertex format,
    /// not combinatorial pipeline variants; derived from `normal` via
    /// `engine_assets::tangent::arbitrary_orthogonal` when no real tangent
    /// data exists (never sampled meaningfully unless a normal map is
    /// actually bound, see `gpu.rs`'s flat-normal default texture — but it
    /// must still be a real vector genuinely perpendicular to `normal`, not
    /// an arbitrary hardcoded axis: a fixed fallback direction can end up
    /// exactly parallel to a real normal, e.g. an axis-aligned box face,
    /// degenerating the shader's Gram-Schmidt step to NaN).
    pub tangent: [f32; 4],
}

/// A valid, non-NaN tangent for a vertex with no real tangent data —
/// derived from its actual normal (`engine_assets::tangent::arbitrary_orthogonal`)
/// rather than a fixed hardcoded direction, which could end up exactly
/// parallel to some real normal (see `Vertex::tangent`'s doc comment).
fn fallback_tangent(normal: [f32; 3]) -> [f32; 4] {
    let t = engine_assets::tangent::arbitrary_orthogonal(Vec3::from(normal));
    [t.x, t.y, t.z, 1.0]
}

pub struct MeshData {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

/// Appends one quad face: `right x up` must equal `normal` (right-handed)
/// so every face winds counter-clockwise as seen from outside the mesh —
/// required for backface culling to remove the correct (interior) faces.
fn push_face(
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u32>,
    center: Vec3,
    right: Vec3,
    up: Vec3,
    normal: Vec3,
    half_size: f32,
) {
    let base = vertices.len() as u32;
    let corners = [
        center - right * half_size - up * half_size,
        center + right * half_size - up * half_size,
        center + right * half_size + up * half_size,
        center - right * half_size + up * half_size,
    ];
    let uvs = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    // `right` is already the tangent direction (UVs are laid out along
    // `right`/`up`); this fn's own invariant — `right x up == normal` — plus
    // the cyclic cross-product identity means `cross(normal, right) == up`
    // always, so handedness is uniformly +1.0.
    let tangent = [right.x, right.y, right.z, 1.0];
    for (corner, uv) in corners.into_iter().zip(uvs) {
        vertices.push(Vertex {
            position: corner.to_array(),
            normal: normal.to_array(),
            uv,
            tangent,
        });
    }
    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

/// A unit cube (extents -0.5..0.5 on every axis), one flat-shaded face per
/// side (24 vertices, not 8) so per-face normals are exact.
pub fn cube() -> MeshData {
    let x = Vec3::X;
    let y = Vec3::Y;
    let z = Vec3::Z;
    let mut vertices = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);
    let faces: [(Vec3, Vec3, Vec3, Vec3); 6] = [
        (x * 0.5, -z, y, x),   // +X
        (-x * 0.5, z, y, -x),  // -X
        (y * 0.5, x, -z, y),   // +Y
        (-y * 0.5, x, z, -y),  // -Y
        (z * 0.5, x, y, z),    // +Z
        (-z * 0.5, -x, y, -z), // -Z
    ];
    for (center, right, up, normal) in faces {
        push_face(&mut vertices, &mut indices, center, right, up, normal, 0.5);
    }
    MeshData { vertices, indices }
}

/// A unit-radius (0.5) smooth-shaded UV sphere — smooth-shaded (unlike
/// `cube()`'s per-face flat shading) because a rolling ball reads visually
/// wrong flat-shaded. Shared vertices per (lat, lon) sample, normals equal
/// to the (normalized) position since it's centered at the origin.
pub fn sphere() -> MeshData {
    const SEGMENTS: u32 = 16; // longitude subdivisions
    const RINGS: u32 = 12; // latitude subdivisions
    const RADIUS: f32 = 0.5;

    let mut vertices = Vec::new();
    for ring in 0..=RINGS {
        let v = ring as f32 / RINGS as f32;
        let phi = v * std::f32::consts::PI; // 0 (top) .. PI (bottom)
        for seg in 0..=SEGMENTS {
            let u = seg as f32 / SEGMENTS as f32;
            let theta = u * std::f32::consts::TAU;
            let dir = Vec3::new(phi.sin() * theta.cos(), phi.cos(), phi.sin() * theta.sin());
            // The analytic tangent along increasing longitude — perpendicular
            // to `dir` everywhere (including the poles: unlike `dir` itself
            // it doesn't depend on the degenerate `phi.sin()` term), so no
            // normalize-of-zero risk.
            let tangent = Vec3::new(-theta.sin(), 0.0, theta.cos());
            vertices.push(Vertex {
                position: (dir * RADIUS).to_array(),
                normal: dir.to_array(),
                uv: [u, v],
                tangent: [tangent.x, tangent.y, tangent.z, 1.0],
            });
        }
    }

    let mut indices = Vec::new();
    let stride = SEGMENTS + 1;
    for ring in 0..RINGS {
        for seg in 0..SEGMENTS {
            let a = ring * stride + seg;
            let b = a + stride;
            // Wound so cross(v1-v0, v2-v0) points outward (verified against
            // this parametrization's actual coordinates, not assumed) —
            // required for `cull_mode: Back` to cull the correct
            // (interior) faces, same requirement `push_face`'s doc comment
            // states for the cube/plane meshes.
            indices.extend_from_slice(&[a, a + 1, b, a + 1, b + 1, b]);
        }
    }

    MeshData { vertices, indices }
}

/// A GPU-skinned vertex: the same position/normal/uv `Vertex` carries, plus
/// up to 4 joint influences (index into a per-draw joint-matrix palette,
/// see ADR-0015) and their blend weights. A separate type from `Vertex`
/// (not `Vertex` with extra always-present fields) so an ordinary,
/// non-skinned mesh's vertex buffer layout — and therefore `mesh_cache`'s
/// existing contents — is completely unaffected by skinning's existence.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SkinnedVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub joints: [u32; 4],
    pub weights: [f32; 4],
    /// See `Vertex::tangent`'s doc comment.
    pub tangent: [f32; 4],
}

pub struct SkinnedMeshData {
    pub vertices: Vec<SkinnedVertex>,
    pub indices: Vec<u32>,
}

/// Joins an imported mesh's plain geometry with its separately-stored
/// skinning data (vertex-index-aligned to `data.positions`, same
/// convention `normals`/`uvs` already use — see ADR-0015) into the
/// interleaved GPU-ready form the skinned render pipeline uses. Returns
/// `RenderError::SkinVertexCountMismatch` if the two don't have the same
/// vertex count, which would mean a mesh/skin pair that didn't actually
/// come from the same glTF primitive import — not a reachable state
/// through `engine import`'s own path, but a corrupt/hand-edited asset
/// store shouldn't be able to crash the process over it, matching every
/// other decode path in this crate.
pub fn from_skinned_asset(
    data: &engine_assets::mesh::MeshData,
    skin: &engine_assets::skin::SkinData,
    tangents: Option<&engine_assets::tangent::TangentData>,
) -> Result<SkinnedMeshData, RenderError> {
    if data.positions.len() != skin.joints.len() {
        return Err(RenderError::SkinVertexCountMismatch {
            mesh: data.positions.len(),
            skin: skin.joints.len(),
        });
    }
    if let Some(t) = tangents {
        if data.positions.len() != t.tangents.len() {
            return Err(RenderError::TangentVertexCountMismatch {
                mesh: data.positions.len(),
                tangent: t.tangents.len(),
            });
        }
    }
    let vertices = (0..data.positions.len())
        .map(|i| SkinnedVertex {
            position: data.positions[i],
            normal: data.normals[i],
            uv: data.uvs[i],
            joints: skin.joints[i].map(u32::from),
            weights: skin.weights[i],
            tangent: tangents.map_or_else(|| fallback_tangent(data.normals[i]), |t| t.tangents[i]),
        })
        .collect();
    Ok(SkinnedMeshData {
        vertices,
        indices: data.indices.clone(),
    })
}

/// Converts an imported `engine-assets` mesh (plain position/normal/uv data)
/// into the interleaved GPU-ready form the render pipeline uses. Returns
/// `RenderError::TangentVertexCountMismatch` for the same corrupt-store
/// reason `from_skinned_asset` checks it.
pub fn from_asset(
    data: &engine_assets::mesh::MeshData,
    tangents: Option<&engine_assets::tangent::TangentData>,
) -> Result<MeshData, RenderError> {
    if let Some(t) = tangents {
        if data.positions.len() != t.tangents.len() {
            return Err(RenderError::TangentVertexCountMismatch {
                mesh: data.positions.len(),
                tangent: t.tangents.len(),
            });
        }
    }
    let vertices = (0..data.positions.len())
        .map(|i| Vertex {
            position: data.positions[i],
            normal: data.normals[i],
            uv: data.uvs[i],
            tangent: tangents.map_or_else(|| fallback_tangent(data.normals[i]), |t| t.tangents[i]),
        })
        .collect();
    Ok(MeshData {
        vertices,
        indices: data.indices.clone(),
    })
}

/// A flat ground-sized quad in the XZ plane at y = 0, facing +Y.
pub fn plane() -> MeshData {
    let mut vertices = Vec::with_capacity(4);
    let mut indices = Vec::with_capacity(6);
    push_face(
        &mut vertices,
        &mut indices,
        Vec3::ZERO,
        Vec3::X,
        -Vec3::Z,
        Vec3::Y,
        2.0,
    );
    MeshData { vertices, indices }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression test for known-issues.md: a mesh/skin pair that doesn't
    /// come from the same glTF primitive import (here, deliberately
    /// mismatched vertex counts) must return a structured `RenderError`,
    /// not panic via `assert_eq!`.
    #[test]
    fn mismatched_vertex_counts_return_an_error_instead_of_panicking() {
        let mesh = engine_assets::mesh::MeshData {
            positions: vec![[0.0, 0.0, 0.0]; 3],
            normals: vec![[0.0, 1.0, 0.0]; 3],
            uvs: vec![[0.0, 0.0]; 3],
            indices: vec![0, 1, 2],
        };
        let skin = engine_assets::skin::SkinData {
            joints: vec![[0, 0, 0, 0]; 2],
            weights: vec![[1.0, 0.0, 0.0, 0.0]; 2],
        };

        let result = from_skinned_asset(&mesh, &skin, None);
        assert!(matches!(
            result,
            Err(RenderError::SkinVertexCountMismatch { mesh: 3, skin: 2 })
        ));
    }

    /// Same corrupt-store scenario as the skin case above, but for a
    /// mesh/tangent pair that doesn't come from the same import.
    #[test]
    fn mismatched_tangent_counts_return_an_error_instead_of_panicking() {
        let mesh = engine_assets::mesh::MeshData {
            positions: vec![[0.0, 0.0, 0.0]; 3],
            normals: vec![[0.0, 1.0, 0.0]; 3],
            uvs: vec![[0.0, 0.0]; 3],
            indices: vec![0, 1, 2],
        };
        let tangents = engine_assets::tangent::TangentData {
            tangents: vec![[1.0, 0.0, 0.0, 1.0]; 2],
        };

        let result = from_asset(&mesh, Some(&tangents));
        assert!(matches!(
            result,
            Err(RenderError::TangentVertexCountMismatch {
                mesh: 3,
                tangent: 2
            })
        ));
    }
}
