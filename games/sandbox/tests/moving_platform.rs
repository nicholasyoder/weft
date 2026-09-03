//! Headless test of the sandbox's own `moving_platform_system` — no window
//! or GPU needed. Drives `Sim::run()` directly, same pattern as
//! `tests/player_control.rs`.

use engine_core::sim::Sim;
use engine_physics::{physics_step, BodyType, Collider, ColliderShape, RigidBody};
use glam::Vec3;
use sandbox::moving_platform::{moving_platform_system, MovingPlatform};

const DT: f32 = 1.0 / 60.0;

fn spawn_platform(sim: &mut Sim, origin: Vec3) -> hecs::Entity {
    sim.world.spawn((
        RigidBody {
            body_type: BodyType::KinematicPositionBased,
            linear_damping: 0.0,
            angular_damping: 0.0,
        },
        Collider {
            shape: ColliderShape::Box {
                half_extents: Vec3::new(1.0, 0.15, 1.0),
            },
            restitution: 0.0,
            friction: 0.5,
            sensor: false,
            membership: 1,
            filter: u32::MAX,
        },
        engine_core::Transform::from_position(origin),
        MovingPlatform {
            origin,
            axis: Vec3::X,
            amplitude: 3.0,
            period: 4.0,
        },
    ))
}

fn platform_x(sim: &Sim, entity: hecs::Entity) -> f32 {
    sim.world
        .get::<&engine_core::Transform>(entity)
        .unwrap()
        .position
        .x
}

#[test]
fn platform_oscillates_along_its_axis_and_returns_to_origin() {
    let mut sim = Sim::new(0, DT);
    // moving_platform must run before physics — see moving_platform_system's
    // doc comment and the scene file's own system-order note.
    sim.scheduler_mut()
        .add_system("moving_platform", moving_platform_system)
        .add_system("physics", physics_step);

    let origin = Vec3::new(4.0, 1.0, 0.0);
    let platform = spawn_platform(&mut sim, origin);

    // A quarter period (1.0s @ 60Hz = 60 ticks) is where sin() peaks at 1.0,
    // so the platform should be near its max +X excursion.
    let quarter_period_ticks = (1.0 / DT).round() as u64;
    sim.run(quarter_period_ticks).unwrap();
    let x_at_quarter = platform_x(&sim, platform);
    assert!(
        (x_at_quarter - (origin.x + 3.0)).abs() < 0.1,
        "expected the platform near its +X extreme at a quarter period, got x={x_at_quarter}"
    );

    // Run the remaining three quarters (4.0s total @ 60Hz = 240 ticks) to
    // complete a full cycle, landing back near the origin.
    let full_period_ticks = (4.0 / DT).round() as u64;
    sim.run(full_period_ticks - quarter_period_ticks).unwrap();
    let x_at_full = platform_x(&sim, platform);
    assert!(
        (x_at_full - origin.x).abs() < 0.15,
        "expected the platform back near its origin after a full period, got x={x_at_full}"
    );
}

#[test]
fn a_platform_with_no_physics_state_is_a_silent_noop() {
    // No "physics" system registered, so PhysicsState is never inserted into
    // Resources — moving_platform_system must no-op, not panic.
    let mut sim = Sim::new(0, DT);
    sim.scheduler_mut()
        .add_system("moving_platform", moving_platform_system);

    let origin = Vec3::new(0.0, 1.0, 0.0);
    let platform = spawn_platform(&mut sim, origin);

    sim.run(30).unwrap();

    assert_eq!(platform_x(&sim, platform), origin.x);
}
