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
