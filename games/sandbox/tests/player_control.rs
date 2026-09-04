//! Headless test of the sandbox's own `player_control_system` — no window
//! or GPU needed, proving the gameplay logic works independent of
//! presentation (ground rule 2: "a window is a consumer of the headless
//! path"). Drives `Sim::step()` directly with an injected `Input`.
//!
//! physics-substrate-plan.md Phase 7 rewrote the player from a force-driven
//! dynamic sphere to a kinematic character controller
//! (`PhysicsState::move_character`) — these tests cover the new mechanic
//! (horizontal movement, a wall blocking movement, jump-then-land resetting
//! vertical speed, no-input leaving the player at rest) rather than the old
//! force/damping behavior.
//!
//! Every player here spawns well above the ground (never exactly at its
//! analytically-computed resting height) and is given time to fall and
//! settle before any resting-height assertion — see the "gotcha" on
//! `PhysicsState::move_character`'s own doc comment
//! (`crates/engine-physics/src/character.rs`): registering a kinematic
//! character's first pose in exact zero-gap contact with a surface hits a
//! degenerate case in rapier's shape-cast TOI solver that can make small
//! per-tick downward moves leak through indefinitely instead of blocking
//! cleanly. A real gap (the normal case — falling under gravity like any
//! other spawn) avoids it entirely.

use engine_core::sim::Sim;
use engine_core::{Input, KeyCode};
use engine_physics::{physics_step, BodyType, Collider, ColliderShape, RigidBody};
use glam::Vec3;
use sandbox::camera_follow::CameraFollow;
use sandbox::player_control::{player_control_system, CharacterVelocity, PlayerControl};

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
            sensor: false,
            membership: 1,
            filter: u32::MAX,
        },
        engine_core::Transform::from_position(Vec3::ZERO),
    ));
}

// Capsule half-extent along up = half_height + radius = 0.8; ground top is
// at y=0.1, so a resting player's center settles close to y=0.9 (rapier's
// own small `offset` skin margin lands it a little above that in practice —
// see the tolerances below, not an exact value).
const SPAWN_Y: f32 = 2.0;
const RESTING_Y: f32 = 0.9;
const RESTING_TOLERANCE: f32 = 0.05;

fn spawn_player(sim: &mut Sim, x: f32, z: f32, control: PlayerControl) -> hecs::Entity {
    sim.world.spawn((
        RigidBody {
            body_type: BodyType::KinematicPositionBased,
            linear_damping: 0.0,
            angular_damping: 0.0,
        },
        Collider {
            shape: ColliderShape::Capsule {
                half_height: 0.5,
                radius: 0.3,
            },
            restitution: 0.0,
            friction: 0.5,
            sensor: false,
            membership: 1,
            filter: u32::MAX,
        },
        engine_core::Transform::from_position(Vec3::new(x, SPAWN_Y, z)),
        control,
        CharacterVelocity::default(),
    ))
}

fn default_control() -> PlayerControl {
    PlayerControl {
        speed: 6.0,
        jump_speed: 7.0,
        gravity: 20.0,
    }
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
    let player = spawn_player(&mut sim, 0.0, 0.0, default_control());

    let mut input = Input::default();
    input.set_held(KeyCode::D, true);
    sim.resources.insert(input);

    sim.run(60).unwrap();

    let x = sim
        .world
        .get::<&engine_core::Transform>(player)
        .unwrap()
        .position
        .x;
    assert!(
        x > 0.0,
        "expected holding D to move the player in +X, got x={x}"
    );
}

#[test]
fn a_wall_blocks_horizontal_movement() {
    let mut sim = Sim::new(0, 1.0 / 60.0);
    sim.scheduler_mut()
        .add_system("player_control", player_control_system)
        .add_system("physics", physics_step);

    spawn_ground(&mut sim);
    sim.world.spawn((
        RigidBody {
            body_type: BodyType::Fixed,
            linear_damping: 0.0,
            angular_damping: 0.0,
        },
        Collider {
            shape: ColliderShape::Box {
                half_extents: Vec3::new(0.1, 5.0, 5.0),
            },
            restitution: 0.0,
            friction: 0.5,
            sensor: false,
            membership: 1,
            filter: u32::MAX,
        },
        engine_core::Transform::from_position(Vec3::new(1.5, 0.0, 0.0)),
    ));
    let player = spawn_player(&mut sim, 0.0, 0.0, default_control());

    let mut input = Input::default();
    input.set_held(KeyCode::D, true);
    sim.resources.insert(input);

    sim.run(90).unwrap();

    let x = sim
        .world
        .get::<&engine_core::Transform>(player)
        .unwrap()
        .position
        .x;
    // Wall face is at x=1.4 (position 1.5, half_extents.x=0.1); the
    // capsule's own 0.3 radius means its center should rest around x=1.1,
    // never reaching the wall's face.
    assert!(
        x < 1.2,
        "expected the wall at x=1.4 (face) to stop the player around x=1.1, got x={x}"
    );
}

#[test]
fn jumping_rises_then_lands_and_resets_vertical_speed_to_rest() {
    let mut sim = Sim::new(0, 1.0 / 60.0);
    sim.scheduler_mut()
        .add_system("player_control", player_control_system)
        .add_system("physics", physics_step);

    spawn_ground(&mut sim);
    let player = spawn_player(&mut sim, 0.0, 0.0, default_control());

    let y = |sim: &Sim| {
        sim.world
            .get::<&engine_core::Transform>(player)
            .unwrap()
            .position
            .y
    };

    // Let the player fall from its spawn height and settle at rest first,
    // so it's genuinely grounded (not just teleported there) before the
    // jump is issued.
    sim.run(60).unwrap();
    assert!(
        (y(&sim) - RESTING_Y).abs() < RESTING_TOLERANCE,
        "expected the player to have settled near y={RESTING_Y}, got y={}",
        y(&sim)
    );
    let settled_y = y(&sim);

    // Tap Space for a single tick — CharacterVelocity's "vertical == 0.0
    // means grounded" rule should let this one tick's jump impulse land.
    let mut jump = Input::default();
    jump.set_held(KeyCode::Space, true);
    sim.resources.insert(jump);
    sim.step().unwrap();
    sim.resources.insert(Input::default());

    // Give the jump a moment to actually lift the player off the ground.
    sim.run(5).unwrap();
    assert!(
        y(&sim) > settled_y + 0.1,
        "expected the player to have risen above its settled height mid-jump, got y={}",
        y(&sim)
    );

    // Let gravity bring it back down and land.
    sim.run(60).unwrap();
    assert!(
        (y(&sim) - settled_y).abs() < RESTING_TOLERANCE,
        "expected the player to land back at its settled height, got y={}",
        y(&sim)
    );

    let vertical_after_landing = sim
        .world
        .get::<&CharacterVelocity>(player)
        .unwrap()
        .vertical;
    assert_eq!(
        vertical_after_landing, 0.0,
        "expected landing to reset CharacterVelocity.vertical to exactly 0.0"
    );
}

#[test]
fn no_input_leaves_the_player_at_rest_horizontally() {
    let mut sim = Sim::new(0, 1.0 / 60.0);
    sim.scheduler_mut()
        .add_system("player_control", player_control_system)
        .add_system("physics", physics_step);

    spawn_ground(&mut sim);
    let player = spawn_player(&mut sim, 0.0, 0.0, default_control());

    // No Input resource inserted at all — player_control_system must
    // no-op horizontally, not panic.
    sim.run(60).unwrap();

    let position = sim
        .world
        .get::<&engine_core::Transform>(player)
        .unwrap()
        .position;
    assert_eq!(position.x, 0.0);
    assert_eq!(position.z, 0.0);
    assert!(
        (position.y - RESTING_Y).abs() < RESTING_TOLERANCE,
        "expected the player to settle at resting height under its own gravity, got y={}",
        position.y
    );
}

fn camera_follow_with_yaw(yaw: f32) -> CameraFollow {
    CameraFollow {
        yaw,
        pitch: 0.0,
        distance: 1.0,
        sensitivity: 0.003,
        pitch_min: -1.2,
        pitch_max: 1.2,
        look_offset: Vec3::ZERO,
    }
}

#[test]
fn camera_yaw_rotates_movement_to_be_camera_relative() {
    let mut sim = Sim::new(0, 1.0 / 60.0);
    sim.scheduler_mut()
        .add_system("player_control", player_control_system)
        .add_system("physics", physics_step);

    spawn_ground(&mut sim);
    let player = spawn_player(&mut sim, 0.0, 0.0, default_control());
    // No `Camera` component needed — `player_control_system` only reads
    // `CameraFollow.yaw`, not the camera's own placement.
    sim.world
        .spawn((camera_follow_with_yaw(std::f32::consts::FRAC_PI_2),));

    let mut input = Input::default();
    input.set_held(KeyCode::W, true);
    sim.resources.insert(input);

    sim.run(30).unwrap();

    let position = sim
        .world
        .get::<&engine_core::Transform>(player)
        .unwrap()
        .position;
    assert!(
        position.x.abs() > 0.5,
        "expected a 90-degree camera yaw to redirect forward (W) onto the X axis, got {position:?}"
    );
    assert!(
        position.z.abs() < 0.1,
        "expected negligible Z movement once camera yaw redirects forward onto X, got {position:?}"
    );
}

#[test]
fn moving_rotates_the_player_to_face_the_movement_direction() {
    let mut sim = Sim::new(0, 1.0 / 60.0);
    sim.scheduler_mut()
        .add_system("player_control", player_control_system)
        .add_system("physics", physics_step);

    spawn_ground(&mut sim);
    let player = spawn_player(&mut sim, 0.0, 0.0, default_control());

    let mut input = Input::default();
    input.set_held(KeyCode::D, true);
    sim.resources.insert(input);

    sim.run(30).unwrap();

    let rotation = sim
        .world
        .get::<&engine_core::Transform>(player)
        .unwrap()
        .rotation;
    let facing = rotation * Vec3::NEG_Z;
    // Holding D alone (no camera entity, yaw defaults to 0.0) moves the
    // player in +X; it should end up facing that same direction.
    assert!(
        facing.dot(Vec3::X) > 0.9,
        "expected the player to face its movement direction (+X), got facing={facing:?}"
    );
}

#[test]
fn no_camera_entity_falls_back_to_world_relative_movement() {
    let mut sim = Sim::new(0, 1.0 / 60.0);
    sim.scheduler_mut()
        .add_system("player_control", player_control_system)
        .add_system("physics", physics_step);

    spawn_ground(&mut sim);
    let player = spawn_player(&mut sim, 0.0, 0.0, default_control());

    let mut input = Input::default();
    input.set_held(KeyCode::W, true);
    sim.resources.insert(input);

    sim.run(30).unwrap();

    let position = sim
        .world
        .get::<&engine_core::Transform>(player)
        .unwrap()
        .position;
    assert!(
        position.z < -0.5,
        "expected W with no camera entity to move in world -Z (today's default), got {position:?}"
    );
}
