use glam::Vec3;
use serde::{Deserialize, Serialize};

/// How a rigid body behaves under simulation.
///
/// Kinematic bodies (driven externally by setting their pose directly,
/// rather than by forces) aren't supported yet — nothing in the engine
/// drives an entity's `Transform` after spawn except physics itself, so
/// there's no concrete consumer for kinematic driving yet. Add it when a
/// real need shows up rather than half-wiring it now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BodyType {
    Dynamic,
    Fixed,
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
/// a scene file (see ADR-0008). Deliberately narrow for Phase 6, mirroring
/// the single-camera (Phase 2) and single-mesh-per-glTF (Phase 3) scoping
/// precedent: box and sphere only. An unrecognized `kind` fails scene load
/// with a normal, structured serde/TOML error, the same as any other
/// malformed component field.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ColliderShape {
    Box { half_extents: Vec3 },
    Sphere { radius: f32 },
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
            ColliderShape::Sphere { .. } => panic!("expected Box"),
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
