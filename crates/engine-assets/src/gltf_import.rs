use std::path::Path;

use glam::{Mat3, Mat4, Vec3};

use crate::error::AssetError;
use crate::mesh::{self, MeshData};
use crate::store::AssetStore;

/// The result of importing a single-mesh, single-primitive glTF file:
/// enough to emit a `MeshRef`/`Material` scene-text-file fragment.
#[derive(Debug)]
pub struct ImportedAsset {
    pub mesh_hash: String,
    pub base_color: [f32; 3],
    pub texture_hash: Option<String>,
}

/// Imports a glTF file's one mesh/one primitive (base color material only —
/// full PBR, multi-primitive meshes, and animation import are out of scope,
/// see ADR-0005) into `store`, returning content hashes ready to paste into
/// a scene file.
pub fn import_gltf(path: &Path, store: &AssetStore) -> Result<ImportedAsset, AssetError> {
    let label = path.display().to_string();
    let (document, buffers, images) =
        gltf::import(path).map_err(|source| AssetError::GltfParseFailed {
            path: label.clone(),
            source,
        })?;

    let mut meshes = document.meshes();
    let mesh = meshes
        .next()
        .ok_or_else(|| unsupported(&label, "glTF file contains no meshes"))?;
    if meshes.next().is_some() {
        return Err(unsupported(
            &label,
            "glTF file contains more than one mesh — only a single mesh/primitive is supported",
        ));
    }

    let mut primitives = mesh.primitives();
    let primitive = primitives
        .next()
        .ok_or_else(|| unsupported(&label, "mesh contains no primitives"))?;
    if primitives.next().is_some() {
        return Err(unsupported(
            &label,
            "mesh contains more than one primitive — only a single mesh/primitive is supported",
        ));
    }

    let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));

    let positions: Vec<[f32; 3]> = reader
        .read_positions()
        .ok_or_else(|| unsupported(&label, "primitive has no POSITION attribute"))?
        .collect();
    let normals: Vec<[f32; 3]> = reader
        .read_normals()
        .ok_or_else(|| unsupported(&label, "primitive has no NORMAL attribute"))?
        .collect();
    let uvs: Vec<[f32; 2]> = match reader.read_tex_coords(0) {
        Some(tex_coords) => tex_coords.into_f32().collect(),
        None => vec![[0.0, 0.0]; positions.len()],
    };
    let indices: Vec<u32> = reader
        .read_indices()
        .ok_or_else(|| unsupported(&label, "primitive is not indexed"))?
        .into_u32()
        .collect();

    // Bake the node's accumulated world transform into the mesh data so the
    // stored mesh already sits in the engine's coordinate space, regardless
    // of the source file's own node hierarchy (e.g. legacy Z-up-to-Y-up
    // correction matrices some exporters emit on a wrapper node).
    let transform =
        node_world_transform_for_mesh(&document, mesh.index()).unwrap_or(Mat4::IDENTITY);
    let normal_matrix = Mat3::from_mat4(transform).inverse().transpose();
    let positions: Vec<[f32; 3]> = positions
        .iter()
        .map(|p| transform.transform_point3(Vec3::from(*p)).to_array())
        .collect();
    let normals: Vec<[f32; 3]> = normals
        .iter()
        .map(|n| {
            normal_matrix
                .mul_vec3(Vec3::from(*n))
                .normalize()
                .to_array()
        })
        .collect();

    let mesh_hash = store.put(&mesh::encode(&MeshData {
        positions,
        normals,
        uvs,
        indices,
    })?)?;

    let material = primitive.material();
    let pbr = material.pbr_metallic_roughness();
    let base_color_factor = pbr.base_color_factor();
    let base_color = [
        base_color_factor[0],
        base_color_factor[1],
        base_color_factor[2],
    ];

    let texture_hash = match pbr.base_color_texture() {
        Some(info) => {
            let image_data = &images[info.texture().source().index()];
            Some(store_embedded_image(image_data, &label, store)?)
        }
        None => None,
    };

    Ok(ImportedAsset {
        mesh_hash,
        base_color,
        texture_hash,
    })
}

fn node_world_transform_for_mesh(document: &gltf::Document, mesh_index: usize) -> Option<Mat4> {
    let scene = document
        .default_scene()
        .or_else(|| document.scenes().next())?;
    scene
        .nodes()
        .find_map(|root| find_mesh_transform(&root, Mat4::IDENTITY, mesh_index))
}

fn find_mesh_transform(node: &gltf::Node, parent: Mat4, mesh_index: usize) -> Option<Mat4> {
    let world = parent * Mat4::from_cols_array_2d(&node.transform().matrix());
    if node.mesh().map(|m| m.index()) == Some(mesh_index) {
        return Some(world);
    }
    node.children()
        .find_map(|child| find_mesh_transform(&child, world, mesh_index))
}

fn store_embedded_image(
    data: &gltf::image::Data,
    label: &str,
    store: &AssetStore,
) -> Result<String, AssetError> {
    use gltf::image::Format;

    let invalid_buffer = || AssetError::GltfUnsupported {
        path: label.to_string(),
        reason: "embedded image has an invalid pixel buffer for its declared format".to_string(),
    };

    let rgba = match data.format {
        Format::R8G8B8A8 => {
            image::RgbaImage::from_raw(data.width, data.height, data.pixels.clone())
                .ok_or_else(invalid_buffer)?
        }
        Format::R8G8B8 => image::DynamicImage::ImageRgb8(
            image::RgbImage::from_raw(data.width, data.height, data.pixels.clone())
                .ok_or_else(invalid_buffer)?,
        )
        .to_rgba8(),
        Format::R8 => image::DynamicImage::ImageLuma8(
            image::GrayImage::from_raw(data.width, data.height, data.pixels.clone())
                .ok_or_else(invalid_buffer)?,
        )
        .to_rgba8(),
        Format::R8G8 => image::DynamicImage::ImageLumaA8(
            image::GrayAlphaImage::from_raw(data.width, data.height, data.pixels.clone())
                .ok_or_else(invalid_buffer)?,
        )
        .to_rgba8(),
        other => {
            return Err(AssetError::GltfUnsupported {
                path: label.to_string(),
                reason: format!("unsupported embedded image pixel format {other:?}"),
            })
        }
    };

    let mut png_bytes = Vec::new();
    rgba.write_to(
        &mut std::io::Cursor::new(&mut png_bytes),
        image::ImageFormat::Png,
    )
    .map_err(|source| AssetError::ImageDecodeFailed {
        path: label.to_string(),
        source,
    })?;
    store.put(&png_bytes)
}

fn unsupported(path: &str, reason: &str) -> AssetError {
    AssetError::GltfUnsupported {
        path: path.to_string(),
        reason: reason.to_string(),
    }
}
