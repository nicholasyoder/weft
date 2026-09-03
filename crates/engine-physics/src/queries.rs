//! Read-only queries against `PhysicsState`'s already-computed rapier
//! narrow-phase/query-pipeline state (sensor overlap, raycasts; the
//! character-controller mechanism lands here in a later phase — see
//! docs/roadmap/physics-substrate-plan.md). Split out from `system.rs`
//! (which owns `PhysicsState`'s definition and the per-tick `physics_step`
//! mutation path) — a second `impl PhysicsState` block here is ordinary
//! Rust; only cross-crate `impl`s are restricted by the orphan rule.

use rapier3d::prelude as rp;

use crate::system::PhysicsState;

/// One narrow, engine-native raycast result — never leaks rapier's own
/// `RayIntersection`/`ColliderHandle` types across the crate boundary, same
/// posture as `overlapping`'s plain `Vec<Entity>`. `distance` is rapier's
/// `time_of_impact`: the parameter along `direction` (not necessarily
/// normalized-distance if `direction` wasn't a unit vector) at which the hit
/// occurred.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RaycastHit {
    pub entity: hecs::Entity,
    pub distance: f32,
    pub point: glam::Vec3,
    pub normal: glam::Vec3,
}

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

    /// Casts a ray from `origin` along `direction` (need not be normalized;
    /// `distance` on a hit is rapier's raw `time_of_impact` along it, so a
    /// non-unit `direction` makes `distance` a parameter, not a physical
    /// distance) up to `max_toi`, optionally excluding one entity's body
    /// (typically the caller's own, so a raycast doesn't hit its origin).
    ///
    /// `solid: true` is passed to rapier unconditionally — an origin that
    /// starts inside a shape reports an immediate `toi=0` hit rather than
    /// passing through to what's behind it. No concrete need yet for the
    /// hollow-shape variant, so it isn't exposed as a parameter.
    ///
    /// Returns `None` on a miss, or if `exclude` names an entity with no
    /// registered body yet — that's a silent no-op (the filter is simply
    /// left unrestricted), matching `overlapping`/`apply_force`'s convention
    /// for unregistered entities rather than treating it as an error.
    pub fn cast_ray(
        &self,
        origin: glam::Vec3,
        direction: glam::Vec3,
        max_toi: f32,
        exclude: Option<hecs::Entity>,
    ) -> Option<RaycastHit> {
        let ray = rp::Ray::new(origin, direction);
        let mut filter = rp::QueryFilter::default();
        if let Some(entity) = exclude {
            if let Some(&handle) = self.bodies.get(&entity) {
                filter = filter.exclude_rigid_body(handle);
            }
        }
        let (collider_handle, hit) = self
            .world
            .cast_ray_and_get_normal(&ray, max_toi, true, filter)?;
        let entity = *self.entity_by_collider.get(&collider_handle)?;
        Some(RaycastHit {
            entity,
            distance: hit.time_of_impact,
            point: ray.point_at(hit.time_of_impact),
            normal: hit.normal,
        })
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

    fn spawn_box(sim: &mut Sim, position: Vec3) -> hecs::Entity {
        sim.world.spawn((
            RigidBody {
                body_type: BodyType::Fixed,
                linear_damping: 0.0,
                angular_damping: 0.0,
            },
            Collider {
                shape: ColliderShape::Box {
                    half_extents: Vec3::ONE,
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
    fn cast_ray_hits_a_fixed_collider_and_reports_entity_distance_point_normal() {
        let mut sim = Sim::new(0, 1.0 / 60.0);
        sim.scheduler_mut()
            .add_system("physics", crate::system::physics_step);
        let target = spawn_box(&mut sim, Vec3::ZERO);
        sim.step().unwrap(); // registers the collider

        let hit = sim
            .resources
            .get::<PhysicsState>()
            .unwrap()
            .cast_ray(
                Vec3::new(0.0, 5.0, 0.0),
                Vec3::new(0.0, -1.0, 0.0),
                100.0,
                None,
            )
            .expect("expected the downward ray to hit the box");

        assert_eq!(hit.entity, target);
        assert!(
            (hit.distance - 4.0).abs() < 1e-4,
            "expected distance 4.0 (origin y=5 to box top y=1), got {}",
            hit.distance
        );
        assert!(
            (hit.point - Vec3::new(0.0, 1.0, 0.0)).length() < 1e-4,
            "expected the hit point at the box's top face, got {:?}",
            hit.point
        );
        assert!(
            (hit.normal - Vec3::new(0.0, 1.0, 0.0)).length() < 1e-4,
            "expected the surface normal to point straight up, got {:?}",
            hit.normal
        );
    }

    #[test]
    fn cast_ray_returns_none_on_a_miss() {
        let mut sim = Sim::new(0, 1.0 / 60.0);
        sim.scheduler_mut()
            .add_system("physics", crate::system::physics_step);
        spawn_box(&mut sim, Vec3::ZERO);
        sim.step().unwrap();

        let hit = sim.resources.get::<PhysicsState>().unwrap().cast_ray(
            Vec3::new(100.0, 5.0, 0.0),
            Vec3::new(0.0, -1.0, 0.0),
            100.0,
            None,
        );
        assert!(hit.is_none(), "expected a ray far from the box to miss");
    }

    #[test]
    fn cast_ray_exclude_skips_the_named_entity() {
        let mut sim = Sim::new(0, 1.0 / 60.0);
        sim.scheduler_mut()
            .add_system("physics", crate::system::physics_step);
        let target = spawn_box(&mut sim, Vec3::ZERO);
        sim.step().unwrap();

        let origin = Vec3::new(0.0, 5.0, 0.0);
        let direction = Vec3::new(0.0, -1.0, 0.0);
        let state = sim.resources.get::<PhysicsState>().unwrap();

        assert!(
            state.cast_ray(origin, direction, 100.0, None).is_some(),
            "sanity check: the ray hits the box when nothing is excluded"
        );
        assert!(
            state
                .cast_ray(origin, direction, 100.0, Some(target))
                .is_none(),
            "expected excluding the only collider in the ray's path to produce a miss"
        );
    }

    #[test]
    fn cast_ray_exclude_of_an_unregistered_entity_is_a_silent_noop() {
        let mut sim = Sim::new(0, 1.0 / 60.0);
        sim.scheduler_mut()
            .add_system("physics", crate::system::physics_step);
        let target = spawn_box(&mut sim, Vec3::ZERO);
        sim.step().unwrap(); // registers `target` only

        // Never given a RigidBody/Collider, so it has no entry in `bodies` —
        // `cast_ray` must not panic and must leave the filter unrestricted.
        let unregistered = sim.world.spawn((Transform::from_position(Vec3::ZERO),));

        let hit = sim
            .resources
            .get::<PhysicsState>()
            .unwrap()
            .cast_ray(
                Vec3::new(0.0, 5.0, 0.0),
                Vec3::new(0.0, -1.0, 0.0),
                100.0,
                Some(unregistered),
            )
            .expect("expected the ray to still hit the box despite the no-op exclude");
        assert_eq!(hit.entity, target);
    }
}
