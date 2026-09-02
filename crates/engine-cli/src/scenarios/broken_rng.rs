//! INTENTIONALLY NONDETERMINISTIC.
//!
//! Exists only to prove the determinism test harness catches ambient RNG
//! use. This is the ONLY file in the workspace permitted to call
//! `rand::thread_rng()` / any ambient RNG source directly instead of
//! threading the seeded RNG through `SystemArgs`.

use engine_core::inspect::ComponentDumper;
use engine_core::scheduler::{SystemArgs, SystemError};
use engine_core::sim::Sim;

use crate::scenarios::basic::Position;

pub fn build(seed: u64) -> Sim {
    let mut sim = Sim::new(seed, 1.0 / 60.0);
    for i in 0..5u32 {
        sim.world.spawn((Position {
            x: i as f32,
            y: 0.0,
            z: 0.0,
        },));
    }
    sim.scheduler_mut()
        .add_system("jitter_ambient", jitter_system_ambient);
    sim
}

fn jitter_system_ambient(args: &mut SystemArgs) -> Result<(), SystemError> {
    use rand::Rng;
    let mut ambient = rand::thread_rng(); // deliberately bypasses args.rng
    for (_e, pos) in args.world.query::<&mut Position>().iter() {
        pos.x += ambient.gen::<f32>() * 0.001;
    }
    Ok(())
}

fn dump_position(e: &hecs::EntityRef) -> Option<(&'static str, serde_json::Value)> {
    e.get::<&Position>()
        .map(|p| ("Position", serde_json::to_value(&*p).unwrap()))
}

pub const DUMPERS: &[ComponentDumper] = &[dump_position];
