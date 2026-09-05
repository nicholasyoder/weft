//! A fourth game-specific component, alongside `player_control`,
//! `camera_follow`, and `hud`'s `Pickup` — a marker the `lever.lua` script
//! (see ADR-0018's `engine.raycast`/`engine.query`/`engine.despawn`) looks
//! up by name to find "the gate entity" without hardcoding an entity id.
//! `scripts/lever.lua` is the first real, live use of `engine.raycast` in
//! `games/sandbox`: holding E only opens the gate when a real line-of-sight
//! ray from the lever to the player actually reports the player as the hit
//! entity, a genuinely different interaction pattern from `pickup.lua`'s
//! `engine.overlapping` proximity check.

use serde::{Deserialize, Serialize};

/// Marks the entity `lever.lua` despawns when the player pulls the lever
/// with a clear line of sight. Carries no data — same empty-braces shape as
/// `hud::Pickup` and for the same reason: serde's derived `Deserialize` for
/// a true unit struct only accepts a bare `null`, not `{}`, and an empty
/// scene-file `[entity.components.Gate]` table round-trips to `{}`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Gate {}

impl engine_cli::registry::Named for Gate {
    const NAME: &'static str = "Gate";
}
