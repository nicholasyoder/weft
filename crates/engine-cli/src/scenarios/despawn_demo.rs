//! Minimal, generic demonstrator for entity despawn (Tier 1 roadmap item;
//! see ADR-0011). Not a permanent lifetime/TTL gameplay feature — scoped
//! narrowly to exercise `world.despawn` through the real CLI/scene surface,
//! including a physics-attached entity so engine-physics's eviction path
//! (`engine_physics::system::physics_step`'s `evict_despawned`) is
//! exercised end to end too. Mirrors `basic.rs`'s shape:
//! component/system defined here, registered into the shared registry
//! (see `crate::registry`) the same way `basic::{Position, Velocity,
//! movement_system}` are, so it's still scene-authorable.

use engine_core::inspect::ComponentDumper;
use engine_core::scheduler::{SystemArgs, SystemError};
use engine_core::sim::Sim;
use engine_core::Transform;
use engine_physics::{physics_step, BodyType, Collider, ColliderShape, RigidBody};
use glam::Vec3;
use serde::{Deserialize, Serialize};

use crate::registry::dump;

/// Ticks remaining before the entity carrying this despawns. Decremented
/// once per tick by `despawn_after_system`; the entity is despawned the
/// tick this reaches zero (so `ticks_remaining = N` despawns after exactly
/// N ticks).
#[derive(Serialize, Deserialize, Clone, Copy)]
pub struct DespawnAfter {
    pub ticks_remaining: u32,
}

/// Registered into `SystemRegistry` as `"despawn-after"`. Must run
/// *before* `"physics"` in scene/scenario system order so a
/// physics-attached despawn is evicted from `PhysicsState` the same tick
/// it happens, rather than a tick later.
pub(crate) fn despawn_after_system(args: &mut SystemArgs) -> Result<(), SystemError> {
    let expired: Vec<hecs::Entity> = args
        .world
        .query::<&mut DespawnAfter>()
        .iter()
        .filter_map(|(entity, d)| {
            d.ticks_remaining = d.ticks_remaining.saturating_sub(1);
            (d.ticks_remaining == 0).then_some(entity)
        })
        .collect();
    for entity in expired {
        let _ = args.world.despawn(entity);
    }
    Ok(())
}

impl crate::registry::Named for DespawnAfter {
    const NAME: &'static str = "DespawnAfter";
}

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
    // Physics-attached entity that despawns mid-flight — exercises
    // engine-physics's eviction path.
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
        DespawnAfter {
            ticks_remaining: 10,
        },
    ));
    // A second, physics-free entity so the entity-count assertion doesn't
    // depend on physics timing at all.
    sim.world.spawn((DespawnAfter { ticks_remaining: 3 },));
    sim.scheduler_mut()
        .add_system("despawn-after", despawn_after_system);
    sim.scheduler_mut().add_system("physics", physics_step);
    sim
}

pub const DUMPERS: &[ComponentDumper] = &[
    dump::<Transform>,
    dump::<RigidBody>,
    dump::<Collider>,
    dump::<DespawnAfter>,
];
