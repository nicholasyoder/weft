//! A small sandbox-local system proving out `PhysicsState::set_kinematic_translation`
//! (physics-substrate-plan.md Phase 1) with a real moving obstacle: an
//! entity that oscillates along an axis on a tick-based sine wave. Zero new
//! engine mechanism — same "external consumer of the registry extension
//! point" posture as `player_control.rs`.

use std::f32::consts::TAU;

use engine_core::scheduler::{SystemArgs, SystemError};
use engine_physics::PhysicsState;
use serde::{Deserialize, Serialize};

/// Marks an entity as a kinematic platform that oscillates along `axis`
/// around `origin`, `amplitude` units in each direction, completing one full
/// cycle every `period` seconds. `origin` is authored explicitly (matching
/// the entity's spawn `Transform.position`) rather than inferred from it, so
/// the system stays a pure function of `(component, tick)` — no first-tick
/// special case, no mutable query.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MovingPlatform {
    pub origin: glam::Vec3,
    #[serde(default = "default_axis")]
    pub axis: glam::Vec3,
    #[serde(default = "default_amplitude")]
    pub amplitude: f32,
    #[serde(default = "default_period")]
    pub period: f32,
}

fn default_axis() -> glam::Vec3 {
    glam::Vec3::X
}

fn default_amplitude() -> f32 {
    3.0
}

fn default_period() -> f32 {
    4.0
}

impl engine_cli::registry::Named for MovingPlatform {
    const NAME: &'static str = "MovingPlatform";
}

/// Drives every `MovingPlatform`-tagged entity's kinematic body to
/// `origin + axis * amplitude * sin(tau * elapsed / period)` each tick. Must
/// be registered *before* "physics" in scene order — same requirement as
/// `player_control_system`, since `set_kinematic_translation` stages a
/// target rapier's `physics_step` consumes that same `world.step()`.
pub fn moving_platform_system(args: &mut SystemArgs) -> Result<(), SystemError> {
    let mut targets: Vec<_> = args
        .world
        .query::<&MovingPlatform>()
        .iter()
        .map(|(entity, platform)| (entity, *platform))
        .collect();
    targets.sort_by_key(|(entity, _)| entity.to_bits());

    let Some(state) = args.resources.get_mut::<PhysicsState>() else {
        return Ok(());
    };

    let elapsed = args.tick as f32 * args.dt;
    for (entity, platform) in targets {
        let phase = TAU * elapsed / platform.period;
        let offset = platform.axis.normalize_or_zero() * platform.amplitude * phase.sin();
        state.set_kinematic_translation(entity, platform.origin + offset);
    }
    Ok(())
}
