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

impl PhysicsState {
    /// Applies a force to `entity`'s dynamic rigid body for this tick only.
    /// Call this from a system registered *before* "physics" in scene order
    /// so the force is still pending when `physics_step` steps the world.
    ///
    /// Rapier's own `add_force` does **not** clear itself after stepping —
    /// its doc comment is explicit that a force "keeps being applied at
    /// every physics step until you change it or clear it." `physics_step`
    /// is what makes this a one-tick-only force: it calls `reset_forces` on
    /// every body right after stepping. Don't call `apply_force` from
    /// anywhere that isn't guaranteed to run before "physics" every tick a
    /// force should be active — there's no other mechanism keeping it
    /// applied.
    ///
    /// Returns `false` (silent no-op, never a panic) if `entity` has no
    /// registered body yet — e.g. its very first tick, before
    /// `physics_step` has run once and lazily registered it.
    pub fn apply_force(&mut self, entity: hecs::Entity, force: glam::Vec3, wake_up: bool) -> bool {
        let Some(&handle) = self.bodies.get(&entity) else {
            return false;
        };
        let Some(body) = self.world.bodies.get_mut(handle) else {
            return false;
        };
        body.add_force(vec3_to_rapier(force), wake_up);
        true
    }
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

/// Removes any rapier body whose owning entity no longer exists in `world`
/// (despawned via `world.despawn` since physics last ran), evicted in
/// `Entity::to_bits()` order per ADR-0002. See ADR-0011.
fn evict_despawned(state: &mut PhysicsState, world: &hecs::World) {
    let mut stale: Vec<hecs::Entity> = state
        .bodies
        .keys()
        .copied()
        .filter(|e| !world.contains(*e))
        .collect();
    stale.sort_by_key(|e| e.to_bits());
    for entity in stale {
        if let Some(handle) = state.bodies.remove(&entity) {
            state.world.remove_body(handle);
        }
    }
}

/// Steps physics by one tick: evicts any despawned entity's body first
/// (see `evict_despawned`/ADR-0011), lazily registers newly-spawned
/// `(RigidBody, Collider, Transform)` entities into the rapier world,
/// advances the simulation by `args.dt`, then writes each non-fixed body's
/// updated pose back to its `Transform`. Registered into `SystemRegistry`
/// as `"physics"`.
pub fn physics_step(args: &mut SystemArgs) {
    let state = args.resources.get_or_insert_with(PhysicsState::default);
    evict_despawned(state, args.world);

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
        .pose(pose)
        .linear_damping(rb.linear_damping)
        .angular_damping(rb.angular_damping);
        let (body_handle, _collider_handle) =
            state.world.insert(body_builder, build_collider(&col));
        state.bodies.insert(entity, body_handle);
    }

    state.world.integration_parameters.dt = args.dt;
    state.world.step();

    for (&entity, &handle) in state.bodies.iter() {
        let Some(body) = state.world.bodies.get_mut(handle) else {
            continue;
        };
        if body.is_fixed() {
            continue;
        }
        if let Ok(mut transform) = args.world.get::<&mut Transform>(entity) {
            transform.position = vec3_from_rapier(body.translation());
            transform.rotation = quat_from_rapier(*body.rotation());
        }
        // rapier does NOT clear a force added via `PhysicsState::apply_force`
        // after stepping — its own doc comment on `reset_forces` is explicit
        // that a user force "keeps being applied at every physics step until
        // you change it or clear it." Without this, a single one-tick
        // `apply_force` call would push the body every subsequent tick
        // forever, not just the tick it was called on — exactly the
        // reported "tapping a key shoots the ball at the nearest wall" bug.
        // Cleared here (not woken by it — `wake_up: false` — clearing an
        // already-zero force is a no-op per `reset_forces`'s own check, so
        // this never spuriously wakes a sleeping body).
        body.reset_forces(false);
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
            Transform::from_position(Vec3::ZERO),
        ));
    }

    fn spawn_ball(sim: &mut Sim, height: f32) -> hecs::Entity {
        sim.world.spawn((
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
    fn apply_force_accelerates_dynamic_body() {
        let mut sim = Sim::new(0, 1.0 / 60.0);
        sim.scheduler_mut().add_system("physics", physics_step);
        let ball = sim.world.spawn((
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
            Transform::from_position(Vec3::new(0.0, 100.0, 0.0)),
        ));

        // physics_step lazily registers a spawned entity into PhysicsState
        // on its first tick — apply_force needs that registration to have
        // happened before it can find a handle for `ball`.
        sim.step();

        let applied = sim
            .resources
            .get_mut::<PhysicsState>()
            .expect("physics_step should have inserted PhysicsState by now")
            .apply_force(ball, Vec3::new(50.0, 0.0, 0.0), true);
        assert!(
            applied,
            "expected apply_force to find the now-registered body"
        );

        sim.step();
        let x = sim.world.get::<&Transform>(ball).unwrap().position.x;
        assert!(
            x > 0.0,
            "expected a positive-X force to move the ball in +X, got x={x}"
        );
    }

    #[test]
    fn apply_force_on_unregistered_entity_is_a_silent_no_op() {
        let mut sim = Sim::new(0, 1.0 / 60.0);
        let ball = sim.world.spawn((
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
            Transform::from_position(Vec3::ZERO),
        ));

        // No `physics_step` has run, so nothing is registered yet.
        let mut state = PhysicsState::default();
        let applied = state.apply_force(ball, Vec3::new(1.0, 0.0, 0.0), true);
        assert!(
            !applied,
            "expected apply_force to no-op before physics_step registers the entity"
        );
    }

    #[test]
    fn apply_force_only_affects_the_tick_it_was_called_on() {
        // Regression test for the actual root cause of a reported bug
        // ("tapping a movement key shoots the ball all the way to the
        // nearest wall"): rapier's `add_force` does NOT clear itself after
        // stepping — its own doc comment on `reset_forces` says a force
        // "keeps being applied at every physics step until you change it
        // or clear it." Without `physics_step` calling `reset_forces` every
        // tick, one `apply_force` call would keep accelerating the body
        // forever, not just for the tick it was called on. `linear_damping`
        // is 0.0 here specifically so this test isolates *that* bug from
        // damping — X speed should be flat, not still rising, once
        // `apply_force` stops being called.
        let mut sim = Sim::new(0, 1.0 / 60.0);
        sim.scheduler_mut().add_system("physics", physics_step);
        let ball = sim.world.spawn((
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
            Transform::from_position(Vec3::new(0.0, 100.0, 0.0)),
        ));

        sim.step(); // registers the body
        sim.resources
            .get_mut::<PhysicsState>()
            .unwrap()
            .apply_force(ball, Vec3::new(200.0, 0.0, 0.0), true);
        sim.step(); // consumes the force for exactly this tick

        let linvel_x = |sim: &Sim| {
            let state = sim.resources.get::<PhysicsState>().unwrap();
            let handle = *state.bodies.get(&ball).unwrap();
            state.world.bodies.get(handle).unwrap().linvel().x
        };
        let speed_after_one_tick = linvel_x(&sim);

        // No more `apply_force` calls — with zero damping, X speed should
        // stay flat (only gravity, purely on Y, is still acting).
        sim.run(10);
        let speed_after_more_ticks = linvel_x(&sim);

        assert!(
            (speed_after_more_ticks - speed_after_one_tick).abs() < 0.01,
            "expected X speed to stay constant once apply_force stopped being \
             called: speed_after_one_tick={speed_after_one_tick}, \
             speed_after_more_ticks={speed_after_more_ticks}"
        );
    }

    #[test]
    fn linear_damping_decelerates_a_body_once_no_force_is_applied() {
        // Regression test for a reported bug: a ball given a brief tap of
        // force would coast at that speed indefinitely (a rolling contact
        // loses very little speed to plain Coulomb friction) instead of
        // slowing down, "shooting" all the way to the nearest wall.
        // `linear_damping` is the fix — assert it actually decelerates a
        // body with no force being applied, not just that it's threaded
        // through to rapier without checking the resulting behavior.
        let mut sim = Sim::new(0, 1.0 / 60.0);
        sim.scheduler_mut().add_system("physics", physics_step);
        let ball = sim.world.spawn((
            RigidBody {
                body_type: BodyType::Dynamic,
                linear_damping: 4.0,
                angular_damping: 0.0,
            },
            Collider {
                shape: ColliderShape::Sphere { radius: 0.5 },
                restitution: 0.0,
                friction: 0.5,
            },
            Transform::from_position(Vec3::new(0.0, 50.0, 0.0)),
        ));

        // Register the body, then give it a one-tick horizontal impulse.
        sim.step();
        sim.resources
            .get_mut::<PhysicsState>()
            .unwrap()
            .apply_force(ball, Vec3::new(200.0, 0.0, 0.0), true);
        sim.step();

        let linvel_x = |sim: &Sim| {
            let state = sim.resources.get::<PhysicsState>().unwrap();
            let handle = *state.bodies.get(&ball).unwrap();
            state.world.bodies.get(handle).unwrap().linvel().x
        };
        let speed_right_after_impulse = linvel_x(&sim);
        assert!(
            speed_right_after_impulse > 0.0,
            "expected the impulse to give the ball positive X speed, got {speed_right_after_impulse}"
        );

        // No more force applied — with damping, speed should drop
        // substantially (an undamped body would keep coasting at roughly
        // the same speed indefinitely).
        sim.run(30);
        let speed_after_coasting = linvel_x(&sim);
        assert!(
            speed_after_coasting < speed_right_after_impulse * 0.5,
            "expected linear_damping to noticeably slow the ball down: \
             speed_right_after_impulse={speed_right_after_impulse}, \
             speed_after_coasting={speed_after_coasting}"
        );
    }

    #[test]
    fn despawning_an_entity_evicts_its_physics_body() {
        let mut sim = Sim::new(0, 1.0 / 60.0);
        sim.scheduler_mut().add_system("physics", physics_step);
        let ball = spawn_ball(&mut sim, 10.0);

        sim.step(); // registers the body
        assert!(
            sim.resources
                .get::<PhysicsState>()
                .unwrap()
                .bodies
                .contains_key(&ball),
            "expected the body to be registered after its first tick"
        );

        sim.world.despawn(ball).unwrap();
        sim.step(); // evict_despawned should now run and remove it

        assert!(
            !sim.resources
                .get::<PhysicsState>()
                .unwrap()
                .bodies
                .contains_key(&ball),
            "expected the despawned entity's body to be evicted"
        );
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
