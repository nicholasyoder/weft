use std::collections::{HashMap, HashSet};
use std::path::Path;

use glam::{Mat3, Mat4, Vec3};

use crate::animation::{AnimationClip, JointTrack, Keyframes};
use crate::error::AssetError;
use crate::mesh::{self, MeshData};
use crate::skeleton::{self, Joint, Skeleton, Trs};
use crate::skin::{self, SkinData};
use crate::store::AssetStore;
use crate::tangent::{self, TangentData};

/// One imported mesh+material pair, ready to emit as its own scene-text-
/// file entity. A glTF file with several mesh nodes and/or several
/// primitives per mesh imports as several `ImportedPart`s (see ADR-0020) —
/// there is no parent/child transform component in this engine yet, so
/// each part's transform is the *whole file's* mesh-local-to-model-root
/// transform baked into its vertex data (same baking ADR-0005 already does
/// for the single-mesh case), and a scene author places the group by
/// giving every part's entity the same `Transform`.
#[derive(Debug)]
pub struct ImportedPart {
    pub mesh_hash: String,
    pub base_color: [f32; 3],
    pub texture_hash: Option<String>,
    pub metallic_factor: f32,
    pub roughness_factor: f32,
    pub metallic_roughness_texture_hash: Option<String>,
    pub normal_texture_hash: Option<String>,
    pub normal_scale: f32,
    pub tangent_hash: Option<String>,
    /// Content hash of this part's own `engine_assets::skin::SkinData` —
    /// `Some` only for a primitive under the file's one skinned mesh node
    /// (see ADR-0015/ADR-0020). Skin data is per-primitive (JOINTS_0/
    /// WEIGHTS_0 are vertex attributes), unlike `skeleton_hash`/`clip_hash`
    /// on `ImportedAsset`, which are shared by every skinned part.
    pub skin_hash: Option<String>,
}

/// The result of importing a glTF file: one `ImportedPart` per mesh
/// primitive reachable from the default scene, plus the skeleton/clip
/// shared by whichever part(s) are skinned (`skin_hash` lives on the part
/// itself — see `ImportedPart`). `skeleton_hash`/`clip_hash` are `Some`
/// only when the file's one skinned mesh node has a glTF skin attached
/// (see ADR-0015/ADR-0020).
#[derive(Debug)]
pub struct ImportedAsset {
    pub parts: Vec<ImportedPart>,
    pub skeleton_hash: Option<String>,
    pub clip_hash: Option<String>,
}

/// Imports every mesh primitive reachable from a glTF file's default scene
/// (see ADR-0020 — multi-mesh/multi-primitive import; previously limited to
/// one mesh/one primitive per ADR-0005). Each primitive becomes its own
/// `ImportedPart` (mesh + metallic-roughness PBR material, ready to paste
/// into a scene file). At most one mesh node in the file may carry a glTF
/// skin; when one does, every primitive under that node also gets skinning
/// data, and the file's (at most one) skeleton/animation clip are imported
/// once and shared by them — see ADR-0015. A mesh referenced by more than
/// one node (instancing), or more than one skinned mesh node, is a
/// structured `ASSET_GLTF_UNSUPPORTED` error rather than silently picking
/// one — the same scope-limiting pattern the per-primitive checks below
/// already use.
pub fn import_gltf(path: &Path, store: &AssetStore) -> Result<ImportedAsset, AssetError> {
    let label = path.display().to_string();
    let (document, buffers, images) =
        gltf::import(path).map_err(|source| AssetError::GltfParseFailed {
            path: label.clone(),
            source,
        })?;

    if document.skins().count() > 1 {
        return Err(unsupported(
            &label,
            "glTF file contains more than one skin — only a single skin is supported",
        ));
    }

    let mesh_nodes = collect_mesh_nodes(&document, &label)?;
    if mesh_nodes.is_empty() {
        return Err(unsupported(&label, "glTF file contains no meshes"));
    }
    if mesh_nodes.iter().filter(|n| n.skin.is_some()).count() > 1 {
        return Err(unsupported(
            &label,
            "more than one skinned mesh node — only a single skin is supported",
        ));
    }

    let mut parts = Vec::new();
    let mut skeleton_hash = None;
    let mut clip_hash = None;

    for mesh_node in &mesh_nodes {
        let mesh = document
            .meshes()
            .nth(mesh_node.mesh_index)
            .expect("mesh_index came from document.meshes()");

        // Per the glTF spec, a skinned mesh's own node transform is ignored
        // for skinning purposes — only joint transforms matter, so the
        // stored vertex data must stay exactly as authored (in bind
        // space), not have the node transform baked in (see ADR-0005
        // decision 4 and ADR-0015). Bake the node's accumulated world
        // transform into the mesh data for an *unskinned* mesh so it
        // already sits in the engine's coordinate space regardless of the
        // source file's own node hierarchy.
        let transform = if mesh_node.skin.is_some() {
            Mat4::IDENTITY
        } else {
            mesh_node.transform
        };
        let normal_matrix = Mat3::from_mat4(transform).inverse().transpose();

        // Joint ordering/skeleton/clip depend only on the skin, not on any
        // one primitive, so compute them once per skinned mesh node and
        // share across every primitive under it.
        let ordering = match &mesh_node.skin {
            Some(skin) => {
                let ordering = JointOrdering::build(&document, skin);
                let skeleton = ordering.build_skeleton(skin, &buffers, &label)?;
                skeleton_hash = Some(store.put(&skeleton::encode(&skeleton)?)?);

                clip_hash = match document.animations().count() {
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

                Some(ordering)
            }
            None => None,
        };

        for (prim_idx, primitive) in mesh.primitives().enumerate() {
            let where_ = format!("mesh {}, primitive {prim_idx}", mesh_node.mesh_index);
            let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));

            let positions: Vec<[f32; 3]> = reader
                .read_positions()
                .ok_or_else(|| {
                    unsupported(
                        &label,
                        &format!("primitive has no POSITION attribute ({where_})"),
                    )
                })?
                .collect();
            let vertex_count = positions.len();
            let normals: Vec<[f32; 3]> = reader
                .read_normals()
                .ok_or_else(|| {
                    unsupported(
                        &label,
                        &format!("primitive has no NORMAL attribute ({where_})"),
                    )
                })?
                .collect();
            let uvs: Vec<[f32; 2]> = match reader.read_tex_coords(0) {
                Some(tex_coords) => tex_coords.into_f32().collect(),
                None => vec![[0.0, 0.0]; positions.len()],
            };
            let indices: Vec<u32> = reader
                .read_indices()
                .ok_or_else(|| {
                    unsupported(&label, &format!("primitive is not indexed ({where_})"))
                })?
                .into_u32()
                .collect();

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

            let metallic_factor = pbr.metallic_factor();
            let roughness_factor = pbr.roughness_factor();
            let metallic_roughness_texture_hash = match pbr.metallic_roughness_texture() {
                Some(info) => {
                    let image_data = &images[info.texture().source().index()];
                    Some(store_embedded_image(image_data, &label, store)?)
                }
                None => None,
            };

            let normal_texture_hash = match material.normal_texture() {
                Some(info) => {
                    let image_data = &images[info.texture().source().index()];
                    Some(store_embedded_image(image_data, &label, store)?)
                }
                None => None,
            };
            let normal_scale = material
                .normal_texture()
                .map(|nt| nt.scale())
                .unwrap_or(1.0);

            // Tangents are only generated/stored when a normal map is
            // actually assigned — no point paying for unused data. Read the
            // glTF file's own TANGENT accessor first (already includes a
            // handedness `w`); when absent, generate from the mesh's own
            // (already-transformed, for the unskinned case) positions/
            // normals/UVs. A file-provided tangent's `xyz` is still in the
            // file's original space, so — for an unskinned mesh — it needs
            // the same rotation `positions`/`normals` above were just baked
            // with; a tangent is an ordinary embedded direction, not a
            // normal, so it uses the plain 3x3 (`transform`'s linear part),
            // not `normal_matrix`'s inverse-transpose.
            let tangent_hash = if normal_texture_hash.is_some() {
                let tangents: Vec<[f32; 4]> = match reader.read_tangents() {
                    Some(iter) => {
                        let rotation = Mat3::from_mat4(transform);
                        iter.map(|t| {
                            let rotated =
                                rotation.mul_vec3(Vec3::new(t[0], t[1], t[2])).normalize();
                            [rotated.x, rotated.y, rotated.z, t[3]]
                        })
                        .collect()
                    }
                    None => tangent::generate(&positions, &normals, &uvs, &indices),
                };
                Some(store.put(&tangent::encode(&TangentData { tangents })?)?)
            } else {
                None
            };

            let mesh_hash = store.put(&mesh::encode(&MeshData {
                positions,
                normals,
                uvs,
                indices,
            })?)?;

            let skin_hash = match (&mesh_node.skin, &ordering) {
                (Some(_), Some(ordering)) => {
                    let joints: Vec<[u16; 4]> = reader
                        .read_joints(0)
                        .ok_or_else(|| {
                            unsupported(
                                &label,
                                &format!(
                                    "skinned mesh primitive has no JOINTS_0 attribute ({where_})"
                                ),
                            )
                        })?
                        .into_u16()
                        .collect();
                    let weights: Vec<[f32; 4]> = reader
                        .read_weights(0)
                        .ok_or_else(|| {
                            unsupported(
                                &label,
                                &format!(
                                    "skinned mesh primitive has no WEIGHTS_0 attribute ({where_})"
                                ),
                            )
                        })?
                        .into_f32()
                        .collect();
                    if joints.len() != vertex_count || weights.len() != vertex_count {
                        return Err(unsupported(
                            &label,
                            &format!("JOINTS_0/WEIGHTS_0 attribute count does not match vertex count ({where_})"),
                        ));
                    }

                    let remapped_joints = joints
                        .into_iter()
                        .map(|j| j.map(|x| ordering.orig_to_new[x as usize]))
                        .collect();
                    let skin_data = SkinData {
                        joints: remapped_joints,
                        weights,
                    };
                    Some(store.put(&skin::encode(&skin_data)?)?)
                }
                _ => None,
            };

            parts.push(ImportedPart {
                mesh_hash,
                base_color,
                texture_hash,
                metallic_factor,
                roughness_factor,
                metallic_roughness_texture_hash,
                normal_texture_hash,
                normal_scale,
                tangent_hash,
                skin_hash,
            });
        }
    }

    Ok(ImportedAsset {
        parts,
        skeleton_hash,
        clip_hash,
    })
}

/// A node reachable from the default scene that references a mesh: which
/// mesh, the node's accumulated world transform, and its glTF skin (if
/// any). Built once per file by `collect_mesh_nodes` and shared by every
/// primitive under that mesh, replacing what used to be a set of
/// single-target tree walks (one per mesh) from the single-mesh era.
struct MeshNode<'a> {
    mesh_index: usize,
    transform: Mat4,
    skin: Option<gltf::Skin<'a>>,
}

/// Walks the default scene once, collecting every node that references a
/// mesh. Rejects (structured `ASSET_GLTF_UNSUPPORTED`) a mesh reachable
/// from more than one node — instancing is not a supported way to reuse a
/// mesh in this engine yet (see ADR-0020's "revisit when").
fn collect_mesh_nodes<'a>(
    document: &'a gltf::Document,
    label: &str,
) -> Result<Vec<MeshNode<'a>>, AssetError> {
    let mut found = Vec::new();
    let mut seen = HashSet::new();
    if let Some(scene) = document
        .default_scene()
        .or_else(|| document.scenes().next())
    {
        for root in scene.nodes() {
            walk_mesh_nodes(&root, Mat4::IDENTITY, &mut found, &mut seen, label)?;
        }
    }
    Ok(found)
}

fn walk_mesh_nodes<'a>(
    node: &gltf::Node<'a>,
    parent: Mat4,
    found: &mut Vec<MeshNode<'a>>,
    seen: &mut HashSet<usize>,
    label: &str,
) -> Result<(), AssetError> {
    let world = parent * Mat4::from_cols_array_2d(&node.transform().matrix());
    if let Some(mesh) = node.mesh() {
        if !seen.insert(mesh.index()) {
            return Err(unsupported(
                label,
                "mesh referenced by more than one node — instancing is not supported",
            ));
        }
        found.push(MeshNode {
            mesh_index: mesh.index(),
            transform: world,
            skin: node.skin(),
        });
    }
    for child in node.children() {
        walk_mesh_nodes(&child, world, found, seen, label)?;
    }
    Ok(())
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
        tracks: tracks_sorted_by_joint(tracks),
    })
}

/// Sorted by joint index: `tracks` is a `HashMap`, whose iteration order is
/// randomized per-process, so an unsorted `.collect()` here made
/// re-importing the identical file produce a different `AnimationClip` byte
/// encoding (and thus a different content hash) on different runs — see
/// docs/roadmap/known-issues.md. `sampling::sample` looks tracks up by
/// `joint` field regardless of `Vec` order, so this sort changes nothing
/// about playback, only makes the encoding stable.
fn tracks_sorted_by_joint(tracks: HashMap<u16, JointTrack>) -> Vec<JointTrack> {
    let mut tracks: Vec<JointTrack> = tracks.into_values().collect();
    tracks.sort_by_key(|t| t.joint);
    tracks
}

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

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_track(joint: u16) -> JointTrack {
        JointTrack {
            joint,
            translation: None,
            rotation: None,
            scale: None,
        }
    }

    #[test]
    fn tracks_sorted_by_joint_is_deterministic_regardless_of_hashmap_order() {
        let mut tracks = HashMap::new();
        // Insertion order intentionally not joint-ascending; a `HashMap`'s
        // own iteration order is randomized per-process regardless, so this
        // test asserts the *output* invariant directly rather than trying
        // to force a particular (unpredictable) input iteration order.
        for joint in [4u16, 0, 3, 1, 2] {
            tracks.insert(joint, empty_track(joint));
        }
        let sorted = tracks_sorted_by_joint(tracks);
        let joints: Vec<u16> = sorted.iter().map(|t| t.joint).collect();
        assert_eq!(joints, vec![0, 1, 2, 3, 4]);
    }
}
