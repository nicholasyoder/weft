//! A second game-specific component/system, alongside `player_control` —
//! keeps the camera positioned relative to whichever entity has
//! `PlayerControl`, rather than reusing engine-render's `Camera` for
//! anything camera-specific (its `target` field already covers look-at;
//! this is purely "where the camera entity's own `Transform` should be").

use engine_core::scheduler::{SystemArgs, SystemError};
use engine_core::Transform;
use engine_render::Camera;
use glam::Vec3;
use serde::{Deserialize, Serialize};

use crate::player_control::PlayerControl;

/// Marks a camera entity as tracking whichever entity has `PlayerControl`
/// (there's exactly one in this game; if that ever changes, the
/// lowest-entity-id one wins, per ADR-0002's stable-iteration-order rule).
/// `offset` is added to the target's position to get the camera's own
/// position; `look_offset` is added to get `Camera.target` (the look-at
/// point) — both fixed relative to the target, not the target's facing,
/// since a rolling ball has no meaningful "forward" direction to chase.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CameraFollow {
    #[serde(default = "default_offset")]
    pub offset: Vec3,
    #[serde(default)]
    pub look_offset: Vec3,
}

fn default_offset() -> Vec3 {
    Vec3::new(0.0, 6.0, 8.0)
}

impl engine_cli::registry::Named for CameraFollow {
    const NAME: &'static str = "CameraFollow";
}

/// Must run *after* "physics" in scene order so it reads the target's
/// post-physics position for this tick, not last tick's (avoiding a
/// one-tick lag between the ball moving and the camera catching up).
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
        transform.position = target_position + follow.offset;
        camera.target = target_position + follow.look_offset;
    }
    Ok(())
}
