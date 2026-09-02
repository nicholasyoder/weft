use glam::Vec3;

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
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
    for (corner, uv) in corners.into_iter().zip(uvs) {
        vertices.push(Vertex {
            position: corner.to_array(),
            normal: normal.to_array(),
            uv,
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
            vertices.push(Vertex {
                position: (dir * RADIUS).to_array(),
                normal: dir.to_array(),
                uv: [u, v],
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
}

pub struct SkinnedMeshData {
    pub vertices: Vec<SkinnedVertex>,
    pub indices: Vec<u32>,
}

/// Joins an imported mesh's plain geometry with its separately-stored
/// skinning data (vertex-index-aligned to `data.positions`, same
/// convention `normals`/`uvs` already use — see ADR-0015) into the
/// interleaved GPU-ready form the skinned render pipeline uses. Panics if
/// the two don't have the same vertex count, which would mean a mesh/skin
/// pair that didn't actually come from the same glTF primitive import —
/// not a reachable state through `engine import`'s own path.
pub fn from_skinned_asset(
    data: &engine_assets::mesh::MeshData,
    skin: &engine_assets::skin::SkinData,
) -> SkinnedMeshData {
    assert_eq!(
        data.positions.len(),
        skin.joints.len(),
        "mesh and skin must have the same vertex count"
    );
    let vertices = (0..data.positions.len())
        .map(|i| SkinnedVertex {
            position: data.positions[i],
            normal: data.normals[i],
            uv: data.uvs[i],
            joints: skin.joints[i].map(u32::from),
            weights: skin.weights[i],
        })
        .collect();
    SkinnedMeshData {
        vertices,
        indices: data.indices.clone(),
    }
}

/// Converts an imported `engine-assets` mesh (plain position/normal/uv data)
/// into the interleaved GPU-ready form the render pipeline uses.
pub fn from_asset(data: &engine_assets::mesh::MeshData) -> MeshData {
    let vertices = (0..data.positions.len())
        .map(|i| Vertex {
            position: data.positions[i],
            normal: data.normals[i],
            uv: data.uvs[i],
        })
        .collect();
    MeshData {
        vertices,
        indices: data.indices.clone(),
    }
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
