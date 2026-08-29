use engine_core::Transform;
use engine_render::{render_scene, Camera, Material, MeshKind, MeshRef};
use glam::Vec3;

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
        },
    )
}

#[test]
fn no_camera_is_a_structured_error_not_a_panic() {
    let mut world = hecs::World::new();
    world.spawn(cube_at(Vec3::ZERO));

    let err = match render_scene(&world, 32, 32) {
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

    let err = match render_scene(&world, 32, 32) {
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

    let a = render_scene(&world, 48, 48).unwrap();
    let b = render_scene(&world, 48, 48).unwrap();
    assert_eq!(a.into_raw(), b.into_raw());
}

#[test]
fn an_empty_scene_with_only_a_camera_renders_the_clear_color() {
    let mut world = hecs::World::new();
    world.spawn(camera_at(Vec3::new(0.0, 0.0, 5.0)));

    let image = render_scene(&world, 16, 16).unwrap();
    // No drawables: every pixel should be exactly the clear color.
    let first = image.get_pixel(0, 0);
    for pixel in image.pixels() {
        assert_eq!(pixel, first);
    }
}
