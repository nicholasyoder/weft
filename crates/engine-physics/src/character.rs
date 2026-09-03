//! Kinematic character controller mechanism: `PhysicsState::move_character`
//! wraps rapier's `control::KinematicCharacterController`, which is what
//! resolves a character's desired movement against the world — sliding
//! along walls, stepping, ground snapping — via its own internal
//! shape-casting. Not re-exported through `rapier3d::prelude`, hence its
//! own `use` below.

use rapier3d::control::KinematicCharacterController;
use rapier3d::prelude as rp;

use crate::system::PhysicsState;

/// One narrow, engine-native character-move result — never leaks rapier's
/// own `EffectiveCharacterMovement`, same posture as `queries::RaycastHit`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CharacterMoveResult {
    pub translation: glam::Vec3,
    pub grounded: bool,
}

impl PhysicsState {
    /// Resolves `entity`'s desired movement against the world using rapier's
    /// kinematic character controller (wall sliding, step handling, ground
    /// snapping, all with rapier's own default tuning — no concrete need
    /// yet for per-scene configuration), then drives the entity's body to
    /// the resulting pose via `set_next_kinematic_translation` — the same
    /// mechanism `set_kinematic_translation` uses, so `physics_step`'s pose
    /// write-back needs no changes to pick it up next tick.
    ///
    /// Requires `entity` to already have a registered body *and* collider
    /// (populated together by `physics_step`'s lazy registration — see
    /// Phase 1). Returns `None` (silent no-op) if either is missing yet,
    /// matching `apply_force`/`overlapping`/`cast_ray`'s convention for
    /// unregistered entities.
    ///
    /// Doesn't check that the body is actually `KinematicPositionBased` or
    /// the collider a capsule — `set_next_kinematic_translation` already
    /// silently no-ops on a non-kinematic body, so there's nothing useful
    /// to add here beyond what rapier itself already guards.
    pub fn move_character(
        &mut self,
        entity: hecs::Entity,
        desired_translation: glam::Vec3,
    ) -> Option<CharacterMoveResult> {
        let &body_handle = self.bodies.get(&entity)?;
        let &collider_handle = self.colliders.get(&entity)?;
        let dt = self.world.integration_parameters.dt;

        let (next_translation, move_result) = {
            let body = self.world.bodies.get(body_handle)?;
            let collider = self.world.colliders.get(collider_handle)?;
            let character_pos = body.position();
            let shape = collider.shape();

            let filter = rp::QueryFilter::default().exclude_rigid_body(body_handle);
            let query_pipeline = self.world.query_pipeline_with_filter(filter);

            let effective = KinematicCharacterController::default().move_shape(
                dt,
                &query_pipeline,
                shape,
                character_pos,
                desired_translation,
                |_| {},
            );

            (
                character_pos.translation + effective.translation,
                CharacterMoveResult {
                    translation: effective.translation,
                    grounded: effective.grounded,
                },
            )
        };

        self.world
            .bodies
            .get_mut(body_handle)?
            .set_next_kinematic_translation(next_translation);
        Some(move_result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{BodyType, Collider, ColliderShape, RigidBody};
    use engine_core::sim::Sim;
    use engine_core::Transform;
    use glam::Vec3;

    fn spawn_floor(sim: &mut Sim) {
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
            Transform::from_position(Vec3::ZERO),
        ));
    }

    fn spawn_wall(sim: &mut Sim, x: f32) {
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
            Transform::from_position(Vec3::new(x, 0.0, 0.0)),
        ));
    }

    fn spawn_character(sim: &mut Sim, position: Vec3) -> hecs::Entity {
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
            Transform::from_position(position),
        ))
    }

    #[test]
    fn move_character_reports_grounded_and_blocks_a_move_into_a_wall() {
        let mut sim = Sim::new(0, 1.0 / 60.0);
        sim.scheduler_mut()
            .add_system("physics", crate::system::physics_step);

        spawn_floor(&mut sim);
        // Capsule half-extent along up = half_height + radius = 0.8; floor
        // top is at y=0.1, so resting center is y=0.9.
        let character = spawn_character(&mut sim, Vec3::new(0.0, 0.9, 0.0));
        spawn_wall(&mut sim, 1.5);

        sim.step().unwrap(); // registers all three bodies/colliders

        let result = sim
            .resources
            .get_mut::<PhysicsState>()
            .unwrap()
            .move_character(character, Vec3::new(10.0, 0.0, 0.0))
            .expect("expected move_character to find the registered character");

        assert!(
            result.grounded,
            "expected the character resting on the floor to report grounded"
        );
        assert!(
            result.translation.x < 5.0,
            "expected the wall to block most of a desired 10.0-unit move, got translation.x={}",
            result.translation.x
        );
        assert!(
            result.translation.x > 0.0,
            "expected the character to still move some distance toward the wall, got translation.x={}",
            result.translation.x
        );
    }

    #[test]
    fn move_character_on_an_unregistered_entity_is_a_silent_noop() {
        let mut sim = Sim::new(0, 1.0 / 60.0);
        let character = sim.world.spawn((Transform::from_position(Vec3::ZERO),));

        // No `physics_step` has run, so nothing is registered yet.
        let mut state = PhysicsState::default();
        let result = state.move_character(character, Vec3::new(1.0, 0.0, 0.0));
        assert!(
            result.is_none(),
            "expected move_character to no-op before physics_step registers the entity"
        );
    }
}
