use std::collections::HashMap;

use engine_core::scheduler::SystemArgs;
use engine_core::Transform;
use rapier3d::prelude as rp;

use crate::components::{BodyType, Collider, ColliderShape, RigidBody};
use crate::convert::{quat_from_rapier, quat_to_rapier, vec3_from_rapier, vec3_to_rapier};

/// Cross-tick rapier state, held in a `Sim`'s `Resources` bag (see
/// ADR-0008) — never touched from a scene file or agent-facing surface
/// directly. Gravity is a fixed `(0, -9.81, 0)` for this phase (rapier's
/// own `PhysicsWorld` default); no concrete need yet for it to be
/// scene-configurable.
#[derive(Default)]
pub struct PhysicsState {
    pub world: rp::PhysicsWorld,
    bodies: HashMap<hecs::Entity, rp::RigidBodyHandle>,
}

fn build_collider(collider: &Collider) -> rp::ColliderBuilder {
    let builder = match collider.shape {
        ColliderShape::Box { half_extents } => {
            rp::ColliderBuilder::cuboid(half_extents.x, half_extents.y, half_extents.z)
        }
        ColliderShape::Sphere { radius } => rp::ColliderBuilder::ball(radius),
    };
    builder
        .restitution(collider.restitution)
        .friction(collider.friction)
}

/// Steps physics by one tick: lazily registers newly-spawned
/// `(RigidBody, Collider, Transform)` entities into the rapier world,
/// advances the simulation by `args.dt`, then writes each non-fixed body's
/// updated pose back to its `Transform`. Registered into `SystemRegistry`
/// as `"physics"`.
///
/// No despawn handling: no entity in this engine is ever despawned yet, so
/// the handle map only grows within a `Sim`'s lifetime (see ADR-0008).
pub fn physics_step(args: &mut SystemArgs) {
    let state = args.resources.get_or_insert_with(PhysicsState::default);

    let new_entities: Vec<_> = args
        .world
        .query::<(&RigidBody, &Collider, &Transform)>()
        .iter()
        .filter(|(entity, _)| !state.bodies.contains_key(entity))
        .map(|(entity, (rb, col, transform))| (entity, *rb, *col, *transform))
        .collect();

    for (entity, rb, col, transform) in new_entities {
        let pose = rp::Pose::from_parts(
            vec3_to_rapier(transform.position),
            quat_to_rapier(transform.rotation),
        );
        let body_builder = match rb.body_type {
            BodyType::Dynamic => rp::RigidBodyBuilder::dynamic(),
            BodyType::Fixed => rp::RigidBodyBuilder::fixed(),
        }
        .pose(pose);
        let (body_handle, _collider_handle) =
            state.world.insert(body_builder, build_collider(&col));
        state.bodies.insert(entity, body_handle);
    }

    state.world.integration_parameters.dt = args.dt;
    state.world.step();

    for (&entity, &handle) in state.bodies.iter() {
        let Some(body) = state.world.bodies.get(handle) else {
            continue;
        };
        if body.is_fixed() {
            continue;
        }
        if let Ok(mut transform) = args.world.get::<&mut Transform>(entity) {
            transform.position = vec3_from_rapier(body.translation());
            transform.rotation = quat_from_rapier(*body.rotation());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_core::sim::Sim;
    use glam::Vec3;

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
            Transform::from_position(Vec3::ZERO),
        ));
    }

    fn spawn_ball(sim: &mut Sim, height: f32) -> hecs::Entity {
        sim.world.spawn((
            RigidBody {
                body_type: BodyType::Dynamic,
            },
            Collider {
                shape: ColliderShape::Sphere { radius: 0.5 },
                restitution: 0.0,
                friction: 0.5,
            },
            Transform::from_position(Vec3::new(0.0, height, 0.0)),
        ))
    }

    #[test]
    fn dynamic_body_falls_under_gravity() {
        let mut sim = Sim::new(0, 1.0 / 60.0);
        sim.scheduler_mut().add_system("physics", physics_step);
        let ball = spawn_ball(&mut sim, 10.0);

        sim.step();
        let after_one_tick = sim.world.get::<&Transform>(ball).unwrap().position.y;
        assert!(
            after_one_tick < 10.0,
            "expected the ball to have fallen, got y={after_one_tick}"
        );
    }

    #[test]
    fn dynamic_body_comes_to_rest_on_fixed_ground() {
        let mut sim = Sim::new(0, 1.0 / 60.0);
        sim.scheduler_mut().add_system("physics", physics_step);
        spawn_ground(&mut sim);
        let ball = spawn_ball(&mut sim, 3.0);

        sim.run(300);

        let resting_y = sim.world.get::<&Transform>(ball).unwrap().position.y;
        // Ground top is at y=0.1, ball radius 0.5: resting center ~= 0.6.
        assert!(
            (resting_y - 0.6).abs() < 0.05,
            "expected the ball to rest around y=0.6, got y={resting_y}"
        );
    }

    #[test]
    fn fixed_body_never_moves() {
        let mut sim = Sim::new(0, 1.0 / 60.0);
        sim.scheduler_mut().add_system("physics", physics_step);
        spawn_ground(&mut sim);
        let ground = sim.world.query::<&RigidBody>().iter().next().unwrap().0;

        sim.run(60);

        let position = sim.world.get::<&Transform>(ground).unwrap().position;
        assert_eq!(position, Vec3::ZERO);
    }

    #[test]
    fn same_seed_and_ticks_produce_identical_output() {
        let run = || {
            let mut sim = Sim::new(0, 1.0 / 60.0);
            sim.scheduler_mut().add_system("physics", physics_step);
            spawn_ground(&mut sim);
            let ball = spawn_ball(&mut sim, 5.0);
            sim.run(120);
            let position = sim.world.get::<&Transform>(ball).unwrap().position;
            position
        };

        assert_eq!(run(), run());
    }
}
