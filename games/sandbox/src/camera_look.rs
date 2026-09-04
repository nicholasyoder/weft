//! Mouse-look: updates every `CameraFollow`'s `yaw`/`pitch` from the live
//! loop's per-frame `MouseDelta` (see `engine_cli::live` / ADR-0010's
//! per-frame-`Resources`-value precedent, same as `Input`). Split out from
//! `camera_follow_system` (which places the camera each tick) so this can
//! run *before* `player_control` — camera-relative movement needs this
//! tick's fresh yaw, not last tick's.
//!
//! `MouseDelta` is never inserted outside `engine play`'s live loop (batch
//! commands stay deterministic per ADR-0002), so this system is a no-op in
//! `test`/`run`/`replay`/`render`/`mix` — `CameraFollow.yaw`/`.pitch` never
//! change there, exactly like `PlayerControl` never moves without a live
//! `Input`.

use engine_core::scheduler::{SystemArgs, SystemError};

use crate::camera_follow::CameraFollow;

pub fn camera_look_system(args: &mut SystemArgs) -> Result<(), SystemError> {
    let Some(delta) = args.resources.get::<engine_core::MouseDelta>().copied() else {
        return Ok(());
    };
    if delta.dx == 0.0 && delta.dy == 0.0 {
        return Ok(());
    }

    for (_, follow) in args.world.query::<&mut CameraFollow>().iter() {
        follow.yaw -= delta.dx * follow.sensitivity;
        // `+=`, not `-=`: confirmed inverted on a real mouse (see this
        // function's own history) — moving the mouse up should pitch the
        // camera up (look down at the target from higher above), not down.
        follow.pitch = (follow.pitch + delta.dy * follow.sensitivity)
            .clamp(follow.pitch_min, follow.pitch_max);
    }
    Ok(())
}
