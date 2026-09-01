//! Headless test of the sandbox's own `player_control_system` — no window
//! or GPU needed, proving the gameplay logic works independent of
//! presentation (ground rule 2: "a window is a consumer of the headless
//! path"). Drives `Sim::step()` directly with an injected `Input`.

use engine_core::sim::Sim;
use engine_core::{Input, KeyCode};
use engine_physics::{physics_step, BodyType, Collider, ColliderShape, RigidBody};
use glam::Vec3;
use sandbox::player_control::{player_control_system, PlayerControl};

fn spawn_ground(sim: &mut Sim) {
    sim.world.spawn((
        RigidBody {
            body_type: BodyType::Fixed,
        },
        Collider {
            shape: ColliderShape::Box {
                half_extents: Vec3::new(50.0, 0.1, 50.0),
            },
            restitution: 0.0,
            friction: 0.5,
        },
        engine_core::Transform::from_position(Vec3::ZERO),
    ));
}

#[test]
fn holding_d_moves_the_player_in_positive_x() {
    let mut sim = Sim::new(0, 1.0 / 60.0);
    // player_control must run before physics — see the doc comment on
    // player_control_system and the scene file's own system-order note.
    sim.scheduler_mut()
        .add_system("player_control", player_control_system)
        .add_system("physics", physics_step);

    spawn_ground(&mut sim);
    let player = sim.world.spawn((
        RigidBody {
            body_type: BodyType::Dynamic,
        },
        Collider {
            shape: ColliderShape::Sphere { radius: 0.5 },
            restitution: 0.1,
            friction: 0.6,
        },
        engine_core::Transform::from_position(Vec3::new(0.0, 2.0, 0.0)),
        PlayerControl { force: 12.0 },
    ));

    let mut input = Input::default();
    input.set_held(KeyCode::D, true);
    sim.resources.insert(input);

    sim.run(30);

    let x = sim
        .world
        .get::<&engine_core::Transform>(player)
        .unwrap()
        .position
        .x;
    assert!(
        x > 0.0,
        "expected holding D to move the ball in +X, got x={x}"
    );
}

#[test]
fn no_input_leaves_the_player_at_rest_horizontally() {
    let mut sim = Sim::new(0, 1.0 / 60.0);
    sim.scheduler_mut()
        .add_system("player_control", player_control_system)
        .add_system("physics", physics_step);

    spawn_ground(&mut sim);
    let player = sim.world.spawn((
        RigidBody {
            body_type: BodyType::Dynamic,
        },
        Collider {
            shape: ColliderShape::Sphere { radius: 0.5 },
            restitution: 0.1,
            friction: 0.6,
        },
        engine_core::Transform::from_position(Vec3::new(0.0, 2.0, 0.0)),
        PlayerControl { force: 12.0 },
    ));

    // No Input resource inserted at all — player_control_system must
    // no-op, not panic.
    sim.run(30);

    let position = sim
        .world
        .get::<&engine_core::Transform>(player)
        .unwrap()
        .position;
    assert_eq!(position.x, 0.0);
    assert_eq!(position.z, 0.0);
}
