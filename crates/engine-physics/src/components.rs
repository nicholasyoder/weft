use glam::Vec3;
use serde::{Deserialize, Serialize};

/// How a rigid body behaves under simulation.
///
/// `KinematicPositionBased` is driven externally: some other system (not
/// physics itself — see `PhysicsState::set_kinematic_translation`) decides
/// an entity's next pose each tick, calling it before "physics" runs.
/// Rapier applies that pose exactly — gravity/forces never touch a
/// non-dynamic body (see `RigidBody::add_force`'s dynamic-only guard) — and
/// `physics_step`'s pose write-back treats it like a dynamic body: whatever
/// pose was set gets copied back into `Transform` every tick, so other
/// systems (camera follow, rendering, …) see the current position.
///
/// Only position-based kinematics are offered. Rapier also has a
/// velocity-based kinematic mode, but every concrete consumer here (moving
/// platforms, a character controller) wants "I decide the exact next
/// pose," not "I set a velocity and let rapier integrate it" — add
/// velocity-based kinematics if a real need shows up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BodyType {
    Dynamic,
    Fixed,
    KinematicPositionBased,
}

/// Marks an entity as physics-simulated. Requires an accompanying
/// [`Collider`] and `engine_core::Transform` (the physics system's initial
/// pose comes from `Transform`, and — for `Dynamic` bodies — is written
/// back to it every tick).
///
/// `linear_damping`/`angular_damping` both default to `0.0` (rapier's own
/// default — a body coasts at constant velocity once moving, since a
/// rolling/sliding contact loses very little speed to simple Coulomb
/// friction). Force-driven gameplay (e.g. a player-controlled ball, see
/// `games/sandbox`) generally wants nonzero damping so releasing input
/// actually decelerates the body instead of letting it coast indefinitely —
/// that's a per-entity, scene-authored tuning choice, not an engine default,
/// since a body that's *supposed* to coast (Phase 6's falling-ball demo)
/// shouldn't quietly change behavior.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RigidBody {
    pub body_type: BodyType,
    #[serde(default)]
    pub linear_damping: f32,
    #[serde(default)]
    pub angular_damping: f32,
}

/// A collider's shape, in engine-native terms — never a raw rapier type in
/// a scene file (see ADR-0008). An unrecognized `kind` fails scene load
/// with a normal, structured serde/TOML error, the same as any other
/// malformed component field. `Capsule` maps to rapier's Y-axis-aligned
/// `ColliderBuilder::capsule_y` — the standard upright-character
/// orientation; no separate axis field, matching `Box`/`Sphere`'s own "no
/// orientation knob" simplicity.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ColliderShape {
    Box { half_extents: Vec3 },
    Sphere { radius: f32 },
    Capsule { half_height: f32, radius: f32 },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Collider {
    pub shape: ColliderShape,
    #[serde(default)]
    pub restitution: f32,
    #[serde(default = "default_friction")]
    pub friction: f32,
}

fn default_friction() -> f32 {
    0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collider_shape_round_trips_through_json() {
        let shape = ColliderShape::Box {
            half_extents: Vec3::new(1.0, 2.0, 3.0),
        };
        let json = serde_json::to_value(shape).unwrap();
        let back: ColliderShape = serde_json::from_value(json).unwrap();
        match back {
            ColliderShape::Box { half_extents } => {
                assert_eq!(half_extents, Vec3::new(1.0, 2.0, 3.0))
            }
            _ => panic!("expected Box"),
        }
    }

    #[test]
    fn collider_shape_capsule_round_trips_through_json() {
        let shape = ColliderShape::Capsule {
            half_height: 1.0,
            radius: 0.3,
        };
        let json = serde_json::to_value(shape).unwrap();
        let back: ColliderShape = serde_json::from_value(json).unwrap();
        match back {
            ColliderShape::Capsule {
                half_height,
                radius,
            } => {
                assert_eq!(half_height, 1.0);
                assert_eq!(radius, 0.3);
            }
            _ => panic!("expected Capsule"),
        }
    }

    #[test]
    fn collider_defaults_friction_when_omitted() {
        let json = serde_json::json!({ "shape": { "kind": "sphere", "radius": 0.5 } });
        let collider: Collider = serde_json::from_value(json).unwrap();
        assert_eq!(collider.friction, 0.5);
        assert_eq!(collider.restitution, 0.0);
    }
}
