//! Headless test of `camera_follow_system` — no window/GPU needed, same
//! posture as `tests/player_control.rs`.

use engine_core::sim::Sim;
use engine_core::Transform;
use engine_render::Camera;
use glam::Vec3;
use sandbox::camera_follow::{camera_follow_system, CameraFollow};
use sandbox::player_control::PlayerControl;

/// Reproduces the camera's original fixed offset ([0, 6, 8] behind and
/// above the target): distance = sqrt(6^2 + 8^2) = 10.0, pitch =
/// atan2(6, 8) ~= 0.6435 rad, yaw = 0.0.
fn default_follow() -> CameraFollow {
    CameraFollow {
        yaw: 0.0,
        pitch: 0.6435,
        distance: 10.0,
        sensitivity: 0.003,
        pitch_min: -1.2,
        pitch_max: 1.2,
        look_offset: Vec3::ZERO,
    }
}

#[test]
fn camera_orbits_the_player_reproducing_the_original_fixed_offset() {
    let mut sim = Sim::new(0, 1.0 / 60.0);
    sim.scheduler_mut()
        .add_system("camera_follow", camera_follow_system);

    let player = sim.world.spawn((
        Transform::from_position(Vec3::new(3.0, 0.5, -2.0)),
        PlayerControl {
            speed: 6.0,
            jump_speed: 7.0,
            gravity: 20.0,
        },
    ));
    let camera = sim.world.spawn((
        Transform::from_position(Vec3::ZERO),
        Camera {
            target: Vec3::ZERO,
            fov_y_degrees: 45.0,
            near: 0.1,
            far: 100.0,
        },
        default_follow(),
    ));

    sim.step().unwrap();

    let player_position = sim.world.get::<&Transform>(player).unwrap().position;
    let camera_transform = sim.world.get::<&Transform>(camera).unwrap();
    let camera_component = sim.world.get::<&Camera>(camera).unwrap();

    assert!(
        (camera_transform.position - (player_position + Vec3::new(0.0, 6.0, 8.0))).length() < 1e-3
    );
    assert_eq!(camera_component.target, player_position);
}

#[test]
fn camera_moves_when_the_player_moves() {
    let mut sim = Sim::new(0, 1.0 / 60.0);
    sim.scheduler_mut()
        .add_system("camera_follow", camera_follow_system);

    let player = sim.world.spawn((
        Transform::from_position(Vec3::ZERO),
        PlayerControl {
            speed: 6.0,
            jump_speed: 7.0,
            gravity: 20.0,
        },
    ));
    let camera = sim.world.spawn((
        Transform::from_position(Vec3::ZERO),
        Camera {
            target: Vec3::ZERO,
            fov_y_degrees: 45.0,
            near: 0.1,
            far: 100.0,
        },
        default_follow(),
    ));

    sim.step().unwrap();
    let camera_position_before = sim.world.get::<&Transform>(camera).unwrap().position;

    sim.world.get::<&mut Transform>(player).unwrap().position = Vec3::new(5.0, 0.5, 0.0);
    sim.step().unwrap();
    let camera_position_after = sim.world.get::<&Transform>(camera).unwrap().position;

    assert_ne!(camera_position_before, camera_position_after);
    assert!(
        (camera_position_after - (Vec3::new(5.0, 0.5, 0.0) + Vec3::new(0.0, 6.0, 8.0))).length()
            < 1e-3
    );
}

#[test]
fn yaw_orbits_the_camera_around_the_player() {
    let mut sim = Sim::new(0, 1.0 / 60.0);
    sim.scheduler_mut()
        .add_system("camera_follow", camera_follow_system);

    sim.world.spawn((
        Transform::from_position(Vec3::ZERO),
        PlayerControl {
            speed: 6.0,
            jump_speed: 7.0,
            gravity: 20.0,
        },
    ));
    let camera = sim.world.spawn((
        Transform::from_position(Vec3::ZERO),
        Camera {
            target: Vec3::ZERO,
            fov_y_degrees: 45.0,
            near: 0.1,
            far: 100.0,
        },
        CameraFollow {
            yaw: std::f32::consts::FRAC_PI_2,
            ..default_follow()
        },
    ));

    sim.step().unwrap();

    let camera_transform = sim.world.get::<&Transform>(camera).unwrap();
    // A quarter-turn around Y should swap which horizontal axis the
    // "behind the player" offset sits on (x <-> z), while distance/height
    // from the pivot stay the same.
    assert!(
        (camera_transform.position - Vec3::new(8.0, 6.0, 0.0)).length() < 1e-2,
        "expected yaw to orbit the camera to roughly [8, 6, 0], got {:?}",
        camera_transform.position
    );
}

#[test]
fn no_player_control_entity_leaves_the_camera_untouched() {
    let mut sim = Sim::new(0, 1.0 / 60.0);
    sim.scheduler_mut()
        .add_system("camera_follow", camera_follow_system);

    let camera = sim.world.spawn((
        Transform::from_position(Vec3::new(1.0, 2.0, 3.0)),
        Camera {
            target: Vec3::new(9.0, 9.0, 9.0),
            fov_y_degrees: 45.0,
            near: 0.1,
            far: 100.0,
        },
        default_follow(),
    ));

    sim.step().unwrap();

    assert_eq!(
        sim.world.get::<&Transform>(camera).unwrap().position,
        Vec3::new(1.0, 2.0, 3.0)
    );
    assert_eq!(
        sim.world.get::<&Camera>(camera).unwrap().target,
        Vec3::new(9.0, 9.0, 9.0)
    );
}
