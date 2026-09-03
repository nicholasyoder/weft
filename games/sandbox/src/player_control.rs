//! The first game-specific (not engine-native) component/system in Weft —
//! the literal point of Phase 8: an external consumer defining its own
//! gameplay code on top of the engine's registry extension point, with zero
//! new engine mechanism (see ROADMAP.md Phase 8 / docs/decisions/0010).
//!
//! physics-substrate-plan.md Phase 7 rewrote the player from a force-driven
//! rolling sphere to a real kinematic character controller
//! (`PhysicsState::move_character`, Phase 4) — the phase that closes the
//! tier doc's "not anything resembling a controller" complaint.

use engine_core::scheduler::{SystemArgs, SystemError};
use engine_core::KeyCode;
use engine_physics::PhysicsState;
use serde::{Deserialize, Serialize};

/// Marks an entity as reading live keyboard input and moving via
/// `PhysicsState::move_character`. `speed`/`jump_speed` are per-second
/// tuning knobs, scene-authored like any other component field;
/// `gravity` is this character's own downward acceleration — kinematic
/// bodies are exempt from rapier's own gravity (see `RigidBody::add_force`'s
/// dynamic-only guard), so nothing else applies it. See `CharacterVelocity`
/// for the per-tick vertical-speed state this drives.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PlayerControl {
    #[serde(default = "default_speed")]
    pub speed: f32,
    #[serde(default = "default_jump_speed")]
    pub jump_speed: f32,
    #[serde(default = "default_gravity")]
    pub gravity: f32,
}

fn default_speed() -> f32 {
    6.0
}

fn default_jump_speed() -> f32 {
    7.0
}

fn default_gravity() -> f32 {
    20.0
}

impl engine_cli::registry::Named for PlayerControl {
    const NAME: &'static str = "PlayerControl";
}

/// A `PlayerControl` entity's own accumulated vertical speed. `0.0` means
/// "resting" — `player_control_system` only allows a jump when this is
/// exactly `0.0` (reset there by the previous tick's landing), which is
/// what prevents double-jumping without needing a separately stored
/// `grounded` flag.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct CharacterVelocity {
    #[serde(default)]
    pub vertical: f32,
}

impl engine_cli::registry::Named for CharacterVelocity {
    const NAME: &'static str = "CharacterVelocity";
}

/// Reads `Input` from `Resources`, computes a WASD horizontal direction plus
/// this tick's vertical speed (gravity accumulation, a jump impulse on
/// Space while grounded), and moves every `PlayerControl`-tagged entity via
/// `PhysicsState::move_character` — wall sliding, step handling, and ground
/// snapping all come from rapier's own character controller (Phase 4). Must
/// be registered *before* "physics" in scene order: `move_character` stages
/// a `set_next_kinematic_translation` call that "physics" consumes when it
/// steps the world.
pub fn player_control_system(args: &mut SystemArgs) -> Result<(), SystemError> {
    let dt = args.dt;

    let (dir, jump_held) = match args.resources.get::<engine_core::Input>() {
        Some(input) => {
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
            (
                dir.try_normalize().unwrap_or(glam::Vec3::ZERO),
                input.is_held(KeyCode::Space),
            )
        }
        None => (glam::Vec3::ZERO, false),
    };

    // Stable iteration order, per ADR-0002 — hecs makes no order guarantee
    // of its own.
    let mut targets: Vec<_> = args
        .world
        .query::<(&PlayerControl, &CharacterVelocity)>()
        .iter()
        .map(|(entity, (pc, cv))| (entity, *pc, cv.vertical))
        .collect();
    targets.sort_by_key(|(entity, ..)| entity.to_bits());

    let mut updates: Vec<(hecs::Entity, f32)> = Vec::with_capacity(targets.len());
    {
        let Some(state) = args.resources.get_mut::<PhysicsState>() else {
            return Ok(());
        };
        for (entity, pc, mut vertical) in targets {
            if jump_held && vertical == 0.0 {
                vertical = pc.jump_speed;
            }
            vertical -= pc.gravity * dt;

            let desired =
                glam::Vec3::new(dir.x * pc.speed * dt, vertical * dt, dir.z * pc.speed * dt);
            if let Some(result) = state.move_character(entity, desired) {
                if result.grounded && vertical < 0.0 {
                    vertical = 0.0;
                }
            }
            updates.push((entity, vertical));
        }
    }

    for (entity, vertical) in updates {
        if let Ok(mut cv) = args.world.get::<&mut CharacterVelocity>(entity) {
            cv.vertical = vertical;
        }
    }
    Ok(())
}
