//! Read-only queries against `PhysicsState`'s already-computed rapier
//! narrow-phase state (sensor overlap today; raycast/character-controller
//! queries land here in later phases — see
//! docs/roadmap/physics-substrate-plan.md). Split out from `system.rs`
//! (which owns `PhysicsState`'s definition and the per-tick `physics_step`
//! mutation path) — a second `impl PhysicsState` block here is ordinary
//! Rust; only cross-crate `impl`s are restricted by the orphan rule.

use crate::system::PhysicsState;

impl PhysicsState {
    /// Every entity whose collider currently overlaps `entity`'s, per
    /// rapier's narrow-phase intersection graph. Rapier itself only tracks
    /// an "intersection pair" (as opposed to an ordinary contact pair) when
    /// at least one of the two colliders involved is a sensor — so this is
    /// inherently a sensor-overlap query; two solid colliders never appear
    /// here regardless of how close they are.
    ///
    /// Filters on the pair iterator's trailing `bool`: rapier's broad phase
    /// creates a tracked pair on AABB overlap *before* narrow-phase confirms
    /// actual shape intersection (`IntersectionPair::new`'s
    /// `intersecting: false` initial value) — that `false` really does show
    /// up for tracked-but-not-touching pairs, so skipping this filter would
    /// report false positives.
    ///
    /// Returns an empty `Vec` (never panics) if `entity` has no registered
    /// collider yet — e.g. its very first tick, before `physics_step` has
    /// lazily registered it — mirroring `apply_force`'s silent-no-op
    /// convention. Result is sorted by `Entity::to_bits()` (ADR-0002) for
    /// deterministic order, matching every other multi-entity iteration in
    /// this crate (`evict_despawned`, `physics_step`'s pose write-back).
    ///
    /// Reflects positions as of this tick's collision-detection pass, which
    /// rapier runs *before* that tick's integration moves anything (see
    /// `PhysicsPipeline::step_inner`) — completely ordinary physics-engine
    /// step semantics, but worth knowing: a body moved this same tick (a
    /// fresh `set_kinematic_translation` call, or ordinary dynamic motion)
    /// won't show up in `overlapping()` results until the *following*
    /// tick's collision detection catches up to its new position.
    pub fn overlapping(&self, entity: hecs::Entity) -> Vec<hecs::Entity> {
        let Some(&handle) = self.colliders.get(&entity) else {
            return Vec::new();
        };
        let mut hits: Vec<hecs::Entity> = self
            .world
            .intersection_pairs_with(handle)
            .filter_map(|(h1, _, h2, _, intersecting)| {
                if !intersecting {
                    return None;
                }
                let other = if h1 == handle { h2 } else { h1 };
                self.entity_by_collider.get(&other).copied()
            })
            .collect();
        hits.sort_by_key(|e| e.to_bits());
        hits
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{BodyType, Collider, ColliderShape, RigidBody};
    use engine_core::sim::Sim;
    use engine_core::Transform;
    use glam::Vec3;

    #[test]
    fn overlapping_reports_sensor_overlaps_and_updates_when_bodies_move_apart() {
        let mut sim = Sim::new(0, 1.0 / 60.0);
        sim.scheduler_mut()
            .add_system("physics", crate::system::physics_step);

        let sensor = sim.world.spawn((
            RigidBody {
                body_type: BodyType::Fixed,
                linear_damping: 0.0,
                angular_damping: 0.0,
            },
            Collider {
                shape: ColliderShape::Sphere { radius: 2.0 },
                restitution: 0.0,
                friction: 0.5,
                sensor: true,
                membership: 1,
                filter: u32::MAX,
            },
            Transform::from_position(Vec3::ZERO),
        ));
        let mover = sim.world.spawn((
            RigidBody {
                body_type: BodyType::KinematicPositionBased,
                linear_damping: 0.0,
                angular_damping: 0.0,
            },
            Collider {
                shape: ColliderShape::Sphere { radius: 0.3 },
                restitution: 0.0,
                friction: 0.5,
                sensor: false,
                membership: 1,
                filter: u32::MAX,
            },
            Transform::from_position(Vec3::new(1.0, 0.0, 0.0)), // well inside the sensor
        ));

        sim.step().unwrap(); // registers both bodies + computes narrow phase at these positions

        let overlaps_of = |sim: &Sim, e: hecs::Entity| {
            sim.resources.get::<PhysicsState>().unwrap().overlapping(e)
        };

        assert_eq!(
            overlaps_of(&sim, sensor),
            vec![mover],
            "expected the sensor to report the overlapping mover"
        );
        assert_eq!(
            overlaps_of(&sim, mover),
            vec![sensor],
            "expected the mover to symmetrically report the overlapping sensor"
        );

        // Move the mover far away. Rapier's narrow phase runs at the START
        // of a step (before that step's integration moves anything — see
        // `PhysicsPipeline::step_inner`'s `detect_collisions` call, which
        // precedes its substep-integration loop), so it lags the kinematic
        // target by one tick: the first `step()` after `set_kinematic_translation`
        // integrates the body to its new position (proving `Transform`
        // already reflects it, same as `kinematic_body_pose_is_driven_by_...`),
        // but narrow phase for THAT step still ran against the pre-move
        // position. A second step is what lets narrow phase evaluate the
        // now-current position and drop the pair.
        sim.resources
            .get_mut::<PhysicsState>()
            .unwrap()
            .set_kinematic_translation(mover, Vec3::new(100.0, 0.0, 0.0));
        sim.step().unwrap();
        sim.step().unwrap();

        assert!(
            overlaps_of(&sim, sensor).is_empty(),
            "expected no overlap once the mover moved far outside the sensor"
        );
    }
}
