use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use engine_assets::{
    animation, import_audio, import_font, import_gltf, import_texture, mesh, skeleton, skin,
    AssetStore,
};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn scratch_store_dir() -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("engine-assets-test-{}-{n}", std::process::id()))
}

fn count_files(dir: &Path) -> usize {
    if !dir.exists() {
        return 0;
    }
    let mut count = 0;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        for entry in std::fs::read_dir(&current).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                stack.push(entry.path());
            } else {
                count += 1;
            }
        }
    }
    count
}

#[test]
fn put_is_idempotent_and_content_addressed() {
    let dir = scratch_store_dir();
    let store = AssetStore::new(&dir);
    let hash_a = store.put(b"hello world").unwrap();
    let hash_b = store.put(b"hello world").unwrap();
    assert_eq!(hash_a, hash_b);
    assert_eq!(count_files(&dir), 1);
    assert_eq!(store.get(&hash_a).unwrap(), b"hello world");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn importing_a_gltf_twice_produces_the_same_mesh_hash() {
    let dir = scratch_store_dir();
    let store = AssetStore::new(&dir);
    let first = import_gltf(&fixture("box.gltf"), &store).unwrap();
    let second = import_gltf(&fixture("box.gltf"), &store).unwrap();
    assert_eq!(first.mesh_hash, second.mesh_hash);
    assert_eq!(first.base_color, [0.8, 0.0, 0.0]);
    assert_eq!(first.texture_hash, None);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn importing_a_textured_gltf_produces_a_texture_hash_and_stores_a_png() {
    let dir = scratch_store_dir();
    let store = AssetStore::new(&dir);
    let imported = import_gltf(&fixture("box_textured.gltf"), &store).unwrap();
    let texture_hash = imported.texture_hash.expect("expected an embedded texture");
    let bytes = store.get(&texture_hash).unwrap();
    let decoded = image::load_from_memory(&bytes).unwrap();
    assert!(decoded.width() > 0 && decoded.height() > 0);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn multi_primitive_gltf_is_a_structured_error() {
    let dir = scratch_store_dir();
    let store = AssetStore::new(&dir);
    let err = import_gltf(&fixture("multi_primitive.gltf"), &store).unwrap_err();
    assert_eq!(err.code(), "ASSET_GLTF_UNSUPPORTED");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn missing_normals_gltf_is_a_structured_error() {
    let dir = scratch_store_dir();
    let store = AssetStore::new(&dir);
    let err = import_gltf(&fixture("missing_normals.gltf"), &store).unwrap_err();
    assert_eq!(err.code(), "ASSET_GLTF_UNSUPPORTED");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn non_indexed_gltf_is_a_structured_error() {
    let dir = scratch_store_dir();
    let store = AssetStore::new(&dir);
    let err = import_gltf(&fixture("non_indexed.gltf"), &store).unwrap_err();
    assert_eq!(err.code(), "ASSET_GLTF_UNSUPPORTED");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn importing_a_loose_image_file_stores_it_as_png() {
    let dir = scratch_store_dir();
    let store = AssetStore::new(&dir);
    let hash = import_texture(&fixture("CesiumLogoFlat.png"), &store).unwrap();
    let bytes = store.get(&hash).unwrap();
    assert_eq!(
        &bytes[0..8],
        b"\x89PNG\r\n\x1a\n",
        "stored bytes should be a PNG"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn importing_a_skinned_gltf_produces_skin_skeleton_and_clip_data() {
    let dir = scratch_store_dir();
    let store = AssetStore::new(&dir);
    let imported = import_gltf(&fixture("skinned.gltf"), &store).unwrap();

    let skin_hash = imported.skin_hash.expect("expected skin data");
    let skeleton_hash = imported.skeleton_hash.expect("expected skeleton data");
    let clip_hash = imported.clip_hash.expect("expected an animation clip");

    // The mesh node's own [5, 0, 0] translation must NOT be baked into the
    // stored vertex positions for a skinned mesh (see ADR-0005 decision 4's
    // skinned-mesh exception, ADR-0015) — positions must match the raw
    // authored quad exactly.
    let mesh_data = mesh::decode(&store.get(&imported.mesh_hash).unwrap()).unwrap();
    assert_eq!(
        mesh_data.positions,
        vec![
            [-0.5, 0.0, 0.0],
            [0.5, 0.0, 0.0],
            [0.5, 1.0, 0.0],
            [-0.5, 1.0, 0.0],
        ]
    );

    let skin_data = skin::decode(&store.get(&skin_hash).unwrap()).unwrap();
    assert_eq!(
        skin_data.joints,
        vec![[0, 0, 0, 0], [0, 0, 0, 0], [1, 0, 0, 0], [1, 0, 0, 0]]
    );
    assert_eq!(skin_data.weights.len(), 4);
    assert_eq!(skin_data.weights[0], [1.0, 0.0, 0.0, 0.0]);

    let skeleton = skeleton::decode(&store.get(&skeleton_hash).unwrap()).unwrap();
    assert_eq!(skeleton.joints.len(), 2);
    assert_eq!(skeleton.joints[0].parent, None);
    assert_eq!(skeleton.joints[1].parent, Some(0));
    assert_eq!(
        skeleton.joints[1].local_rest_transform.translation,
        [0.0, 1.0, 0.0]
    );
    assert_eq!(
        skeleton.joints[1].inverse_bind_matrix,
        [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, -1.0, 0.0, 1.0],
        ]
    );

    let clip = animation::decode(&store.get(&clip_hash).unwrap()).unwrap();
    assert_eq!(clip.duration, 1.0);
    assert_eq!(clip.tracks.len(), 1);
    assert_eq!(clip.tracks[0].joint, 1);
    assert!(clip.tracks[0].translation.is_none());
    assert!(clip.tracks[0].scale.is_none());
    let rotation = clip.tracks[0].rotation.as_ref().unwrap();
    assert_eq!(rotation.times, vec![0.0, 1.0]);
    assert_eq!(rotation.values[0], [0.0, 0.0, 0.0, 1.0]);
    let sin45 = std::f32::consts::FRAC_1_SQRT_2;
    assert!((rotation.values[1][2] - sin45).abs() < 1e-5);
    assert!((rotation.values[1][3] - sin45).abs() < 1e-5);

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn importing_an_unskinned_gltf_leaves_skin_fields_none() {
    let dir = scratch_store_dir();
    let store = AssetStore::new(&dir);
    let imported = import_gltf(&fixture("box.gltf"), &store).unwrap();
    assert!(imported.skin_hash.is_none());
    assert!(imported.skeleton_hash.is_none());
    assert!(imported.clip_hash.is_none());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn importing_a_font_twice_produces_the_same_hash_and_no_new_files() {
    let dir = scratch_store_dir();
    let store = AssetStore::new(&dir);
    let first = import_font(&fixture("sample.ttf"), &store).unwrap();
    assert_eq!(count_files(&dir), 1);
    let second = import_font(&fixture("sample.ttf"), &store).unwrap();
    assert_eq!(
        first, second,
        "re-importing an unchanged font should be a no-op"
    );
    assert_eq!(count_files(&dir), 1);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn importing_a_font_stores_its_bytes_verbatim() {
    let dir = scratch_store_dir();
    let store = AssetStore::new(&dir);
    let original = std::fs::read(fixture("sample.ttf")).unwrap();
    let hash = import_font(&fixture("sample.ttf"), &store).unwrap();
    let stored = store.get(&hash).unwrap();
    assert_eq!(
        stored, original,
        "font import shouldn't transform the bytes at all — that's engine-render's job, at draw time"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn importing_an_audio_file_twice_produces_the_same_hash_and_no_new_files() {
    let dir = scratch_store_dir();
    let store = AssetStore::new(&dir);
    let first = import_audio(&fixture("sample.wav"), &store).unwrap();
    assert_eq!(count_files(&dir), 1);
    let second = import_audio(&fixture("sample.wav"), &store).unwrap();
    assert_eq!(
        first, second,
        "re-importing an unchanged audio file should be a no-op"
    );
    assert_eq!(count_files(&dir), 1);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn importing_an_audio_file_stores_its_bytes_verbatim() {
    let dir = scratch_store_dir();
    let store = AssetStore::new(&dir);
    let original = std::fs::read(fixture("sample.wav")).unwrap();
    let hash = import_audio(&fixture("sample.wav"), &store).unwrap();
    let stored = store.get(&hash).unwrap();
    assert_eq!(
        stored, original,
        "audio import shouldn't transform the bytes at all — that's engine-audio's job, at play/mix time"
    );
    std::fs::remove_dir_all(&dir).ok();
}
