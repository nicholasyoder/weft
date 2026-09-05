//! A third game-specific component/system, alongside `player_control` and
//! `camera_follow` — counts remaining `Pickup`-tagged entities each tick and
//! writes a "Pickups: N/total" string into the scene's `Text` entity. This
//! is `games/sandbox`'s concrete proof for Weft's text-rendering/minimal-UI
//! layer (see ADR-0014): before this, the player had no on-screen feedback
//! for anything happening in the game.
//!
//! `Pickup` is a marker the game itself adds to the entities it wants
//! counted, rather than the counter reaching into `Script`/`pickup.lua`
//! internals to infer what's collectible — counting via a game-owned
//! component keeps the count correct regardless of how a pickup happens to
//! be implemented.

use engine_core::scheduler::{SystemArgs, SystemError};
use engine_render::Text;
use serde::{Deserialize, Serialize};

/// Marks an entity as one of the collectibles the HUD counter tracks.
/// Carries no data — `pickup.lua` despawns the whole entity on collection,
/// so "still exists" already means "not yet collected." Empty braces, not a
/// true unit struct: serde's derived `Deserialize` for a unit struct only
/// accepts a bare `null`, not `{}` — an empty scene-file `[entity.components.
/// Pickup]` table round-trips to an empty JSON object, which needs this
/// shape to deserialize at all.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Pickup {}

impl engine_cli::registry::Named for Pickup {
    const NAME: &'static str = "Pickup";
}

/// Total pickups `playground.toml` starts with — the counter's denominator.
/// A fixed constant (rather than derived from the scene) is fine for this
/// one hand-authored scene; deriving it would be the natural next step if a
/// second scene ever needs this system.
const TOTAL_PICKUPS: usize = 5;

/// Counts remaining `Pickup`-tagged entities and writes
/// `"Pickups: {collected}/{TOTAL_PICKUPS}"` into every `Text` entity's
/// `content` (`playground.toml` has exactly one). Must run after script
/// dispatch to reflect this tick's collections without lag — `engine-cli`'s
/// live loop dispatches scripts once per tick right after the scene's
/// `[[system]]` list runs, so a one-tick lag here (reading the *previous*
/// tick's despawns) is unavoidable with a plain system, the same lag
/// category `camera_follow_system` already accepts reading post-physics
/// transforms one step later than the event that caused them.
pub fn hud_system(args: &mut SystemArgs) -> Result<(), SystemError> {
    let remaining = args.world.query::<&Pickup>().iter().count();
    let collected = TOTAL_PICKUPS.saturating_sub(remaining);

    for (_, text) in args.world.query::<&mut Text>().iter() {
        text.content = format!("Pickups: {collected}/{TOTAL_PICKUPS}");
    }
    Ok(())
}
