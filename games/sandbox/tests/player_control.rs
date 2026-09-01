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
            linear_damping: 0.0,
            angular_damping: 0.0,
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
            linear_damping: 4.0,
            angular_damping: 4.0,
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
fn releasing_the_key_lets_the_ball_decelerate() {
    // Regression test for a reported bug: tapping D briefly sent the ball
    // shooting all the way to the far wall instead of stopping once D was
    // released. The primary root cause was that rapier's `add_force` never
    // clears itself — a single tick's force call kept being reapplied every
    // subsequent tick forever, since nothing called `reset_forces` (fixed in
    // `physics_step`; see the lower-level regression test
    // `apply_force_only_affects_the_tick_it_was_called_on` in
    // `engine-physics`). `RigidBody.linear_damping`/`angular_damping`
    // (also added by this fix) is what makes the ball actually slow down
    // and stop after release, rather than coast at whatever speed it had —
    // this test asserts that user-visible symptom, measuring instantaneous
    // per-tick speed (not cumulative distance, which is misleading here:
    // the ball is still accelerating from rest during the "held" window, so
    // it naturally covers *less* ground early on than it does coasting at
    // already-built-up speed right after release).
    let mut sim = Sim::new(0, 1.0 / 60.0);
    sim.scheduler_mut()
        .add_system("player_control", player_control_system)
        .add_system("physics", physics_step);

    spawn_ground(&mut sim);
    let player = sim.world.spawn((
        RigidBody {
            body_type: BodyType::Dynamic,
            linear_damping: 4.0,
            angular_damping: 4.0,
        },
        Collider {
            shape: ColliderShape::Sphere { radius: 0.5 },
            restitution: 0.1,
            friction: 0.6,
        },
        engine_core::Transform::from_position(Vec3::new(0.0, 2.0, 0.0)),
        PlayerControl { force: 12.0 },
    ));

    let x = |sim: &Sim| {
        sim.world
            .get::<&engine_core::Transform>(player)
            .unwrap()
            .position
            .x
    };

    // Hold D long enough to approach a steady speed under the (damped)
    // driving force, then measure the last tick's displacement as a proxy
    // for "speed while held."
    let mut held = Input::default();
    held.set_held(KeyCode::D, true);
    sim.resources.insert(held);
    sim.run(19);
    let x_before_last_held_tick = x(&sim);
    sim.run(1);
    let speed_while_held = x(&sim) - x_before_last_held_tick;

    // Release, let damping act for a while, then measure the same way.
    sim.resources.insert(Input::default());
    sim.run(29);
    let x_before_last_coast_tick = x(&sim);
    sim.run(1);
    let speed_after_coasting = x(&sim) - x_before_last_coast_tick;

    assert!(
        speed_after_coasting < speed_while_held * 0.5,
        "expected the ball to be moving much slower after coasting with D \
         released than it was while D was held: speed_while_held={speed_while_held}, \
         speed_after_coasting={speed_after_coasting}"
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
            linear_damping: 4.0,
            angular_damping: 4.0,
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
