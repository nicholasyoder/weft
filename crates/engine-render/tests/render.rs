use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use engine_core::Transform;
use engine_render::{render_scene, Camera, Material, MeshKind, MeshRef, Text};
use glam::Vec3;

fn scratch_assets_dir() -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "engine-render-test-assets-{}-{n}",
        std::process::id()
    ))
}

fn camera_at(position: Vec3) -> (Transform, Camera) {
    (
        Transform::from_position(position),
        Camera {
            target: Vec3::ZERO,
            fov_y_degrees: 45.0,
            near: 0.1,
            far: 100.0,
        },
    )
}

fn cube_at(position: Vec3) -> (Transform, MeshRef, Material) {
    (
        Transform::from_position(position),
        MeshRef {
            mesh: MeshKind::Cube,
        },
        Material {
            color: [0.8, 0.2, 0.2],
            texture: None,
        },
    )
}

#[test]
fn no_camera_is_a_structured_error_not_a_panic() {
    let mut world = hecs::World::new();
    world.spawn(cube_at(Vec3::ZERO));

    let assets_dir = scratch_assets_dir();
    let err = match render_scene(&world, 32, 32, &assets_dir) {
        Err(e) => e,
        Ok(_) => panic!("expected an error"),
    };
    assert_eq!(err.code(), "RENDER_NO_CAMERA");
}

#[test]
fn multiple_cameras_is_a_structured_error_not_a_panic() {
    let mut world = hecs::World::new();
    world.spawn(camera_at(Vec3::new(0.0, 0.0, 5.0)));
    world.spawn(camera_at(Vec3::new(0.0, 0.0, -5.0)));

    let assets_dir = scratch_assets_dir();
    let err = match render_scene(&world, 32, 32, &assets_dir) {
        Err(e) => e,
        Ok(_) => panic!("expected an error"),
    };
    assert_eq!(err.code(), "RENDER_MULTIPLE_CAMERAS");
}

#[test]
fn rendering_the_same_world_twice_is_byte_identical() {
    let mut world = hecs::World::new();
    world.spawn(camera_at(Vec3::new(2.0, 2.0, 4.0)));
    world.spawn(cube_at(Vec3::ZERO));
    world.spawn((Text {
        content: "hello".to_string(),
        x: 4.0,
        y: 4.0,
        size: 16.0,
        color: [1.0, 1.0, 1.0],
        font: None,
    },));

    let assets_dir = scratch_assets_dir();
    let a = render_scene(&world, 48, 48, &assets_dir).unwrap();
    let b = render_scene(&world, 48, 48, &assets_dir).unwrap();
    assert_eq!(a.into_raw(), b.into_raw());
}

#[test]
fn rendering_text_with_the_default_font_produces_non_blank_output() {
    let mut world = hecs::World::new();
    world.spawn(camera_at(Vec3::new(2.0, 2.0, 4.0)));
    world.spawn((Text {
        content: "Weft".to_string(),
        x: 2.0,
        y: 2.0,
        size: 24.0,
        color: [1.0, 1.0, 1.0],
        font: None,
    },));

    let assets_dir = scratch_assets_dir();
    let image = render_scene(&world, 64, 64, &assets_dir).unwrap();
    let clear = image.get_pixel(63, 63);
    assert!(
        image.pixels().any(|p| p != clear),
        "expected the default-font text to draw something other than the clear color"
    );
}

#[test]
fn rendering_text_with_an_imported_custom_font_produces_non_blank_output() {
    let assets_dir = scratch_assets_dir();
    let store = engine_assets::AssetStore::new(&assets_dir);
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../engine-assets/tests/fixtures/sample.ttf");
    let font_hash = engine_assets::import_font(&fixture, &store).unwrap();

    let mut world = hecs::World::new();
    world.spawn(camera_at(Vec3::new(2.0, 2.0, 4.0)));
    world.spawn((Text {
        content: "Weft".to_string(),
        x: 2.0,
        y: 2.0,
        size: 24.0,
        color: [1.0, 1.0, 1.0],
        font: Some(font_hash),
    },));

    let image = render_scene(&world, 64, 64, &assets_dir).unwrap();
    let clear = image.get_pixel(63, 63);
    assert!(
        image.pixels().any(|p| p != clear),
        "expected the custom-font text to draw something other than the clear color"
    );
    std::fs::remove_dir_all(&assets_dir).ok();
}

#[test]
fn rendering_text_with_an_unknown_font_hash_is_a_structured_error() {
    let mut world = hecs::World::new();
    world.spawn(camera_at(Vec3::new(2.0, 2.0, 4.0)));
    world.spawn((Text {
        content: "Weft".to_string(),
        x: 2.0,
        y: 2.0,
        size: 24.0,
        color: [1.0, 1.0, 1.0],
        font: Some("does-not-exist".to_string()),
    },));

    let assets_dir = scratch_assets_dir();
    let err = render_scene(&world, 16, 16, &assets_dir).unwrap_err();
    assert_eq!(err.code(), "RENDER_ASSET_ERROR");
}

#[test]
fn an_empty_scene_with_only_a_camera_renders_the_clear_color() {
    let mut world = hecs::World::new();
    world.spawn(camera_at(Vec3::new(0.0, 0.0, 5.0)));

    let assets_dir = scratch_assets_dir();
    let image = render_scene(&world, 16, 16, &assets_dir).unwrap();
    // No drawables: every pixel should be exactly the clear color.
    let first = image.get_pixel(0, 0);
    for pixel in image.pixels() {
        assert_eq!(pixel, first);
    }
}

#[test]
fn rendering_an_imported_textured_mesh_produces_non_blank_output() {
    let assets_dir = scratch_assets_dir();
    let store = engine_assets::AssetStore::new(&assets_dir);
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../engine-assets/tests/fixtures/box_textured.gltf");
    let imported = engine_assets::import_gltf(&fixture, &store).unwrap();

    let mut world = hecs::World::new();
    world.spawn(camera_at(Vec3::new(2.0, 2.0, 4.0)));
    world.spawn((
        Transform::from_position(Vec3::ZERO),
        MeshRef {
            mesh: MeshKind::Asset(imported.mesh_hash),
        },
        Material {
            color: imported.base_color,
            texture: imported.texture_hash,
        },
    ));

    let image = render_scene(&world, 48, 48, &assets_dir).unwrap();
    let clear = image.get_pixel(0, 0);
    assert!(
        image.pixels().any(|p| p != clear),
        "expected the imported mesh to draw something other than the clear color"
    );
    std::fs::remove_dir_all(&assets_dir).ok();
}

#[test]
fn rendering_a_scene_with_an_unknown_asset_hash_is_a_structured_error() {
    let assets_dir = scratch_assets_dir();
    let mut world = hecs::World::new();
    world.spawn(camera_at(Vec3::new(2.0, 2.0, 4.0)));
    world.spawn((
        Transform::from_position(Vec3::ZERO),
        MeshRef {
            mesh: MeshKind::Asset("does-not-exist".to_string()),
        },
        Material {
            color: [1.0, 1.0, 1.0],
            texture: None,
        },
    ));

    let err = render_scene(&world, 16, 16, &assets_dir).unwrap_err();
    assert_eq!(err.code(), "RENDER_ASSET_ERROR");
}
