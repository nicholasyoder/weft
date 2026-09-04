//! A second game-specific component/system, alongside `player_control` —
//! keeps the camera orbiting whichever entity has `PlayerControl`, rather
//! than reusing engine-render's `Camera` for anything camera-specific (its
//! `target` field already covers look-at; this is purely "where the camera
//! entity's own `Transform` should be").
//!
//! `CameraFollow` used to store a fixed world-space `offset`/`look_offset`
//! pair (a static chase camera). It's now a spherical orbit around the
//! target — `yaw`/`pitch`/`distance` — driven each tick by
//! `camera_look_system` (mouse input) and consumed here to place the
//! camera. Both systems share the same target: `look_offset` is now the
//! shared orbit pivot (target position + `look_offset`), not an
//! independent look-at point.

use engine_core::scheduler::{SystemArgs, SystemError};
use engine_core::Transform;
use engine_render::Camera;
use glam::{Quat, Vec3};
use serde::{Deserialize, Serialize};

use crate::player_control::PlayerControl;

/// Marks a camera entity as orbiting whichever entity has `PlayerControl`
/// (there's exactly one in this game; if that ever changes, the
/// lowest-entity-id one wins, per ADR-0002's stable-iteration-order rule).
/// `yaw`/`pitch` are updated by `camera_look_system` from live mouse
/// motion; `camera_follow_system` (this module) turns them into a camera
/// position each tick. `look_offset` is added to the target's position to
/// get the shared orbit pivot (both the look-at point and the center the
/// camera orbits around).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CameraFollow {
    #[serde(default)]
    pub yaw: f32,
    #[serde(default = "default_pitch")]
    pub pitch: f32,
    #[serde(default = "default_distance")]
    pub distance: f32,
    #[serde(default = "default_sensitivity")]
    pub sensitivity: f32,
    #[serde(default = "default_pitch_min")]
    pub pitch_min: f32,
    #[serde(default = "default_pitch_max")]
    pub pitch_max: f32,
    #[serde(default = "default_look_offset")]
    pub look_offset: Vec3,
}

// Defaults reproduce the camera's original fixed offset ([0, 6, 8] behind
// and above the target): distance = sqrt(6^2 + 8^2) = 10.0, pitch =
// atan2(6, 8) ~= 0.6435 rad.
fn default_pitch() -> f32 {
    0.6435
}

fn default_distance() -> f32 {
    10.0
}

fn default_sensitivity() -> f32 {
    0.003
}

// Kept clear of +/-90 degrees (~1.2 rad ~= 68.8 degrees): gpu.rs's
// look_at_mat4 uses a fixed Vec3::Y up vector, which degenerates when the
// camera-to-target direction is parallel to it.
fn default_pitch_min() -> f32 {
    -1.2
}

fn default_pitch_max() -> f32 {
    1.2
}

fn default_look_offset() -> Vec3 {
    Vec3::new(0.0, 0.5, 0.0)
}

impl engine_cli::registry::Named for CameraFollow {
    const NAME: &'static str = "CameraFollow";
}

/// Must run *after* "physics" in scene order so it reads the target's
/// post-physics position for this tick, not last tick's (avoiding a
/// one-tick lag between the player moving and the camera catching up).
/// Must run *after* "camera_look" too, so it places the camera using this
/// tick's freshly-updated yaw/pitch rather than last tick's.
pub fn camera_follow_system(args: &mut SystemArgs) -> Result<(), SystemError> {
    let mut targets: Vec<_> = args
        .world
        .query::<(&PlayerControl, &Transform)>()
        .iter()
        .map(|(entity, (_, transform))| (entity, transform.position))
        .collect();
    targets.sort_by_key(|(entity, _)| entity.to_bits());
    let Some((_, target_position)) = targets.into_iter().next() else {
        return Ok(());
    };

    for (_, (follow, transform, camera)) in args
        .world
        .query::<(&CameraFollow, &mut Transform, &mut Camera)>()
        .iter()
    {
        let pivot = target_position + follow.look_offset;
        // `-pitch`: a positive pitch should raise the camera above the
        // target (matching the "looking down from above" convention this
        // field's own doc comment/defaults assume), which is the opposite
        // sign of `Quat::from_rotation_x`'s own right-hand-rule direction
        // for a vector starting at `+Z`.
        let rotation = Quat::from_rotation_y(follow.yaw) * Quat::from_rotation_x(-follow.pitch);
        transform.position = pivot + rotation * (Vec3::Z * follow.distance);
        camera.target = pivot;
    }
    Ok(())
}
