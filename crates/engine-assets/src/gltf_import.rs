use std::collections::HashMap;
use std::path::Path;

use glam::{Mat3, Mat4, Vec3};

use crate::animation::{AnimationClip, JointTrack, Keyframes};
use crate::error::AssetError;
use crate::mesh::{self, MeshData};
use crate::skeleton::{self, Joint, Skeleton, Trs};
use crate::skin::{self, SkinData};
use crate::store::AssetStore;

/// The result of importing a single-mesh, single-primitive glTF file:
/// enough to emit a `MeshRef`/`Material`/`Animator` scene-text-file
/// fragment. `skin_hash`/`skeleton_hash`/`clip_hash` are `Some` only when
/// the mesh's node has a glTF skin attached (see `import_gltf`'s doc
/// comment and ADR-0015).
#[derive(Debug)]
pub struct ImportedAsset {
    pub mesh_hash: String,
    pub base_color: [f32; 3],
    pub texture_hash: Option<String>,
    pub skin_hash: Option<String>,
    pub skeleton_hash: Option<String>,
    pub clip_hash: Option<String>,
}

/// Imports a glTF file's one mesh/one primitive (base color material only —
/// full PBR and multi-primitive meshes are out of scope, see ADR-0005) into
/// `store`, returning content hashes ready to paste into a scene file. If
/// the mesh's node has a glTF skin attached, also imports skinning data, a
/// joint skeleton, and (at most one) animation clip — see ADR-0015. At most
/// one skin and one animation are supported per file; anything more is a
/// structured `ASSET_GLTF_UNSUPPORTED` error, the same scope-limiting
/// pattern the mesh/primitive count checks below already use.
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
    let vertex_count = positions.len();
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

    if document.skins().count() > 1 {
        return Err(unsupported(
            &label,
            "glTF file contains more than one skin — only a single skin is supported",
        ));
    }

    let mesh_node = find_mesh_node(&document, mesh.index());
    let skin = mesh_node.as_ref().and_then(|n| n.skin());

    // Per the glTF spec, a skinned mesh's own node transform is ignored for
    // skinning purposes — only joint transforms matter, so the stored
    // vertex data must stay exactly as authored (in bind space), not have
    // the node transform baked in (see ADR-0005 decision 4 and ADR-0015).
    // Bake the node's accumulated world transform into the mesh data for an
    // *unskinned* mesh so it already sits in the engine's coordinate space
    // regardless of the source file's own node hierarchy (e.g. legacy
    // Z-up-to-Y-up correction matrices some exporters emit on a wrapper
    // node).
    let transform = if skin.is_some() {
        Mat4::IDENTITY
    } else {
        node_world_transform_for_mesh(&document, mesh.index()).unwrap_or(Mat4::IDENTITY)
    };
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

    let (skin_hash, skeleton_hash, clip_hash) = match skin {
        Some(skin) => {
            let joints: Vec<[u16; 4]> = reader
                .read_joints(0)
                .ok_or_else(|| {
                    unsupported(&label, "skinned mesh primitive has no JOINTS_0 attribute")
                })?
                .into_u16()
                .collect();
            let weights: Vec<[f32; 4]> = reader
                .read_weights(0)
                .ok_or_else(|| {
                    unsupported(&label, "skinned mesh primitive has no WEIGHTS_0 attribute")
                })?
                .into_f32()
                .collect();
            if joints.len() != vertex_count || weights.len() != vertex_count {
                return Err(unsupported(
                    &label,
                    "JOINTS_0/WEIGHTS_0 attribute count does not match vertex count",
                ));
            }

            let ordering = JointOrdering::build(&document, &skin);
            let remapped_joints = joints
                .into_iter()
                .map(|j| j.map(|x| ordering.orig_to_new[x as usize]))
                .collect();
            let skin_data = SkinData {
                joints: remapped_joints,
                weights,
            };
            let skeleton = ordering.build_skeleton(&skin, &buffers, &label)?;

            let skin_hash = store.put(&skin::encode(&skin_data)?)?;
            let skeleton_hash = store.put(&skeleton::encode(&skeleton)?)?;

            let clip_hash = match document.animations().count() {
                0 => None,
                1 => {
                    let animation = document.animations().next().expect("count() == 1");
                    let clip = build_animation_clip(&animation, &buffers, &ordering, &label)?;
                    Some(store.put(&crate::animation::encode(&clip)?)?)
                }
                _ => {
                    return Err(unsupported(
                        &label,
                        "glTF file contains more than one animation — only a single animation clip is supported",
                    ))
                }
            };

            (Some(skin_hash), Some(skeleton_hash), clip_hash)
        }
        None => (None, None, None),
    };

    Ok(ImportedAsset {
        mesh_hash,
        base_color,
        texture_hash,
        skin_hash,
        skeleton_hash,
        clip_hash,
    })
}

/// Maps a glTF skin's joint nodes into `Skeleton`'s required root-first
/// order (`joints[i].parent < i` always) — `skin.joints()` itself carries
/// no such ordering guarantee.
struct JointOrdering {
    /// `orig_to_new[original skin.joints() index] = root-first index`. Used
    /// to remap the raw `JOINTS_0` vertex attribute (which indexes into
    /// `skin.joints()` in its original order) into `Skeleton`'s order.
    orig_to_new: Vec<u16>,
    /// The original `skin.joints()` index at each root-first position.
    new_to_orig: Vec<usize>,
    /// Each root-first joint's parent, already in root-first indices —
    /// `Joint.parent` is read straight off this.
    parent_new: Vec<Option<u16>>,
    /// glTF node index -> root-first joint index, for mapping an animation
    /// channel's target node to a joint.
    node_to_new: HashMap<usize, u16>,
}

impl JointOrdering {
    fn build(document: &gltf::Document, skin: &gltf::Skin) -> Self {
        let joint_nodes: Vec<gltf::Node> = skin.joints().collect();
        let node_to_orig: HashMap<usize, usize> = joint_nodes
            .iter()
            .enumerate()
            .map(|(orig, n)| (n.index(), orig))
            .collect();
        let parents = build_node_parent_map(document);

        // The nearest ancestor of `orig`'s node that is itself a joint in
        // this skin, expressed as an original `skin.joints()` index.
        let joint_parent_orig = |orig: usize| -> Option<usize> {
            let mut current = joint_nodes[orig].index();
            while let Some(&p) = parents.get(&current) {
                if let Some(&parent_orig) = node_to_orig.get(&p) {
                    return Some(parent_orig);
                }
                current = p;
            }
            None
        };

        let mut children: Vec<Vec<usize>> = vec![Vec::new(); joint_nodes.len()];
        let mut roots = Vec::new();
        for orig in 0..joint_nodes.len() {
            match joint_parent_orig(orig) {
                Some(parent_orig) => children[parent_orig].push(orig),
                None => roots.push(orig),
            }
        }

        // Stable BFS from roots (in original list order), so a parent
        // always lands at an earlier root-first position than its children.
        let mut new_to_orig = Vec::with_capacity(joint_nodes.len());
        let mut parent_new = Vec::with_capacity(joint_nodes.len());
        let mut queue: std::collections::VecDeque<(usize, Option<u16>)> =
            roots.into_iter().map(|orig| (orig, None)).collect();
        while let Some((orig, parent)) = queue.pop_front() {
            let new_idx = new_to_orig.len() as u16;
            new_to_orig.push(orig);
            parent_new.push(parent);
            for &child_orig in &children[orig] {
                queue.push_back((child_orig, Some(new_idx)));
            }
        }

        let mut orig_to_new = vec![0u16; joint_nodes.len()];
        for (new, &orig) in new_to_orig.iter().enumerate() {
            orig_to_new[orig] = new as u16;
        }
        let node_to_new = joint_nodes
            .iter()
            .enumerate()
            .map(|(orig, n)| (n.index(), orig_to_new[orig]))
            .collect();

        Self {
            orig_to_new,
            new_to_orig,
            parent_new,
            node_to_new,
        }
    }

    fn build_skeleton(
        &self,
        skin: &gltf::Skin,
        buffers: &[gltf::buffer::Data],
        label: &str,
    ) -> Result<Skeleton, AssetError> {
        let joint_nodes: Vec<gltf::Node> = skin.joints().collect();
        let reader = skin.reader(|b| Some(&buffers[b.index()]));
        let identity = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let inverse_bind_matrices: Vec<[[f32; 4]; 4]> = match reader.read_inverse_bind_matrices() {
            Some(iter) => iter.collect(),
            None => vec![identity; joint_nodes.len()],
        };
        if inverse_bind_matrices.len() != joint_nodes.len() {
            return Err(unsupported(
                label,
                "skin's inverseBindMatrices count does not match its joint count",
            ));
        }

        let mut joints = Vec::with_capacity(joint_nodes.len());
        for (new_idx, &orig) in self.new_to_orig.iter().enumerate() {
            let node = &joint_nodes[orig];
            let (translation, rotation, scale) = node.transform().decomposed();
            joints.push(Joint {
                parent: self.parent_new[new_idx],
                inverse_bind_matrix: inverse_bind_matrices[orig],
                local_rest_transform: Trs {
                    translation,
                    rotation,
                    scale,
                },
            });
        }
        Ok(Skeleton { joints })
    }
}

fn build_animation_clip(
    animation: &gltf::Animation,
    buffers: &[gltf::buffer::Data],
    joints: &JointOrdering,
    label: &str,
) -> Result<AnimationClip, AssetError> {
    use gltf::animation::util::ReadOutputs;

    let mut tracks: HashMap<u16, JointTrack> = HashMap::new();
    let mut duration = 0.0f32;

    for channel in animation.channels() {
        let target_node_index = channel.target().node().index();
        let joint = *joints.node_to_new.get(&target_node_index).ok_or_else(|| {
            unsupported(
                label,
                "animation channel targets a node that is not a skeleton joint",
            )
        })?;

        let reader = channel.reader(|b| Some(&buffers[b.index()]));
        let times: Vec<f32> = reader
            .read_inputs()
            .ok_or_else(|| unsupported(label, "animation channel has no input accessor"))?
            .collect();
        if let Some(&last) = times.last() {
            duration = duration.max(last);
        }
        let outputs = reader
            .read_outputs()
            .ok_or_else(|| unsupported(label, "animation channel has no output accessor"))?;

        let track = tracks.entry(joint).or_insert_with(|| JointTrack {
            joint,
            translation: None,
            rotation: None,
            scale: None,
        });
        match outputs {
            ReadOutputs::Translations(iter) => {
                track.translation = Some(Keyframes {
                    times,
                    values: iter.collect(),
                });
            }
            ReadOutputs::Rotations(rotations) => {
                track.rotation = Some(Keyframes {
                    times,
                    values: rotations.into_f32().collect(),
                });
            }
            ReadOutputs::Scales(iter) => {
                track.scale = Some(Keyframes {
                    times,
                    values: iter.collect(),
                });
            }
            ReadOutputs::MorphTargetWeights(_) => {
                return Err(unsupported(
                    label,
                    "morph target weight animation is not supported",
                ))
            }
        }
    }

    Ok(AnimationClip {
        duration,
        tracks: tracks.into_values().collect(),
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

/// Finds the node referencing `mesh_index`, if any is reachable from the
/// default scene — used to check for a `skin()` attached to that node.
fn find_mesh_node(document: &gltf::Document, mesh_index: usize) -> Option<gltf::Node<'_>> {
    let scene = document
        .default_scene()
        .or_else(|| document.scenes().next())?;
    scene
        .nodes()
        .find_map(|root| find_mesh_node_under(root, mesh_index))
}

fn find_mesh_node_under(node: gltf::Node<'_>, mesh_index: usize) -> Option<gltf::Node<'_>> {
    if node.mesh().map(|m| m.index()) == Some(mesh_index) {
        return Some(node);
    }
    node.children()
        .find_map(|child| find_mesh_node_under(child, mesh_index))
}

/// Maps every node's index to its parent's index, for the whole document's
/// default-scene node hierarchy — the glTF format only stores child lists,
/// never parent pointers, so this has to be built by walking down once.
fn build_node_parent_map(document: &gltf::Document) -> HashMap<usize, usize> {
    let mut parents = HashMap::new();
    if let Some(scene) = document
        .default_scene()
        .or_else(|| document.scenes().next())
    {
        for root in scene.nodes() {
            walk_node_parents(&root, &mut parents);
        }
    }
    parents
}

fn walk_node_parents(node: &gltf::Node, parents: &mut HashMap<usize, usize>) {
    for child in node.children() {
        parents.insert(child.index(), node.index());
        walk_node_parents(&child, parents);
    }
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
