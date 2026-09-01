//! Hardcoded demo proving gravity + collision end-to-end (Phase 6's DoD):
//! a dynamic ball falls onto a fixed ground plane and comes to rest. Mirrors
//! `basic.rs`'s shape, but reuses the shared `Transform`/`RigidBody`/
//! `Collider` components and dump fns from `crate::registry` rather than
//! redefining scenario-local ones, since these are the same components a
//! scene file would use (see `tests/fixtures/scenes/physics_demo.toml`).

use engine_core::inspect::ComponentDumper;
use engine_core::sim::Sim;
use engine_core::Transform;
use engine_physics::{physics_step, BodyType, Collider, ColliderShape, RigidBody};
use glam::Vec3;

use crate::registry::{dump_collider, dump_rigid_body, dump_transform};

pub fn build(seed: u64) -> Sim {
    let mut sim = Sim::new(seed, 1.0 / 60.0);
    sim.world.spawn((
        Transform::from_position(Vec3::ZERO),
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
    ));
    sim.world.spawn((
        Transform::from_position(Vec3::new(0.0, 5.0, 0.0)),
        RigidBody {
            body_type: BodyType::Dynamic,
            linear_damping: 0.0,
            angular_damping: 0.0,
        },
        Collider {
            shape: ColliderShape::Sphere { radius: 0.5 },
            restitution: 0.0,
            friction: 0.5,
        },
    ));
    sim.scheduler_mut().add_system("physics", physics_step);
    sim
}

pub const DUMPERS: &[ComponentDumper] = &[dump_transform, dump_rigid_body, dump_collider];
