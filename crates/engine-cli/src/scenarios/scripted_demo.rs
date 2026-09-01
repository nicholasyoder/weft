//! Scenario-local component for the RNG/despawn/query Lua-scripting demo
//! scene (ADR-0012). Unlike `despawn_demo.rs`, there is deliberately no
//! matching hardcoded `Scenario` here: a `Script` component's `path` is a
//! filesystem path resolved at dispatch time, which only has a natural,
//! stable home next to a scene *file*
//! (`tests/fixtures/scenes/scripted_gameplay.toml` and its sibling
//! `tests/fixtures/scripts/`), not a hardcoded-in-the-binary scenario.

use serde::{Deserialize, Serialize};

/// Countdown driven entirely from Lua (`fuse.lua`), not a native system —
/// unlike `DespawnAfter`, nothing in engine-cli decrements this. `-1` is the
/// "not yet rolled" sentinel the script uses to pick a random countdown via
/// `engine.random_int` on its first tick.
#[derive(Serialize, Deserialize, Clone, Copy)]
pub struct Fuse {
    pub ticks_remaining: i64,
}

pub(crate) fn dump_fuse(e: &hecs::EntityRef) -> Option<(&'static str, serde_json::Value)> {
    e.get::<&Fuse>()
        .map(|f| ("Fuse", serde_json::to_value(*f).unwrap()))
}
