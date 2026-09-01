//! Headless test of `camera_follow_system` — no window/GPU needed, same
//! posture as `tests/player_control.rs`.

use engine_core::sim::Sim;
use engine_core::Transform;
use engine_render::Camera;
use glam::Vec3;
use sandbox::camera_follow::{camera_follow_system, CameraFollow};
use sandbox::player_control::PlayerControl;

#[test]
fn camera_tracks_the_player_with_a_fixed_offset() {
    let mut sim = Sim::new(0, 1.0 / 60.0);
    sim.scheduler_mut()
        .add_system("camera_follow", camera_follow_system);

    let player = sim.world.spawn((
        Transform::from_position(Vec3::new(3.0, 0.5, -2.0)),
        PlayerControl { force: 12.0 },
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
            offset: Vec3::new(0.0, 6.0, 8.0),
            look_offset: Vec3::ZERO,
        },
    ));

    sim.step();

    let player_position = sim.world.get::<&Transform>(player).unwrap().position;
    let camera_transform = sim.world.get::<&Transform>(camera).unwrap();
    let camera_component = sim.world.get::<&Camera>(camera).unwrap();

    assert_eq!(
        camera_transform.position,
        player_position + Vec3::new(0.0, 6.0, 8.0)
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
        PlayerControl { force: 12.0 },
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
            offset: Vec3::new(0.0, 6.0, 8.0),
            look_offset: Vec3::ZERO,
        },
    ));

    sim.step();
    let camera_position_before = sim.world.get::<&Transform>(camera).unwrap().position;

    sim.world.get::<&mut Transform>(player).unwrap().position = Vec3::new(5.0, 0.5, 0.0);
    sim.step();
    let camera_position_after = sim.world.get::<&Transform>(camera).unwrap().position;

    assert_ne!(camera_position_before, camera_position_after);
    assert_eq!(
        camera_position_after,
        Vec3::new(5.0, 0.5, 0.0) + Vec3::new(0.0, 6.0, 8.0)
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
        CameraFollow {
            offset: Vec3::new(0.0, 6.0, 8.0),
            look_offset: Vec3::ZERO,
        },
    ));

    sim.step();

    assert_eq!(
        sim.world.get::<&Transform>(camera).unwrap().position,
        Vec3::new(1.0, 2.0, 3.0)
    );
    assert_eq!(
        sim.world.get::<&Camera>(camera).unwrap().target,
        Vec3::new(9.0, 9.0, 9.0)
    );
}
