//! The first game-specific (not engine-native) component/system in Weft —
//! the literal point of Phase 8: an external consumer defining its own
//! gameplay code on top of the engine's registry extension point, with zero
//! new engine mechanism (see ROADMAP.md Phase 8 / docs/decisions/0010).

use engine_core::scheduler::{SystemArgs, SystemError};
use engine_core::KeyCode;
use engine_physics::PhysicsState;
use serde::{Deserialize, Serialize};

/// Marks an entity as reading live keyboard input and getting a movement
/// force applied to its rigid body each tick. `force` is a per-entity tuning
/// knob, scene-authored like any other component field.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PlayerControl {
    #[serde(default = "default_force")]
    pub force: f32,
}

fn default_force() -> f32 {
    12.0
}

impl engine_cli::registry::Named for PlayerControl {
    const NAME: &'static str = "PlayerControl";
}

/// Reads `Input` from `Resources`, computes a WASD direction, and applies a
/// force to every `PlayerControl`-tagged entity's rigid body. Must be
/// registered *before* "physics" in scene order — `PhysicsState::apply_force`
/// sets a pending force rapier's `physics_step` clears every `world.step()`.
pub fn player_control_system(args: &mut SystemArgs) -> Result<(), SystemError> {
    let Some(input) = args.resources.get::<engine_core::Input>() else {
        return Ok(());
    };

    let mut dir = glam::Vec3::ZERO;
    if input.is_held(KeyCode::W) {
        dir += glam::Vec3::NEG_Z;
    }
    if input.is_held(KeyCode::S) {
        dir += glam::Vec3::Z;
    }
    if input.is_held(KeyCode::A) {
        dir += glam::Vec3::NEG_X;
    }
    if input.is_held(KeyCode::D) {
        dir += glam::Vec3::X;
    }
    let dir = dir.try_normalize().unwrap_or(glam::Vec3::ZERO);

    // Stable iteration order, per ADR-0002 — hecs makes no order guarantee
    // of its own.
    let mut targets: Vec<_> = args
        .world
        .query::<&PlayerControl>()
        .iter()
        .map(|(entity, pc)| (entity, pc.force))
        .collect();
    targets.sort_by_key(|(entity, _)| entity.to_bits());

    if dir == glam::Vec3::ZERO {
        return Ok(());
    }
    let Some(state) = args.resources.get_mut::<PhysicsState>() else {
        return Ok(());
    };
    for (entity, force) in targets {
        state.apply_force(entity, dir * force, true);
    }
    Ok(())
}
