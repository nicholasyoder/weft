use engine_core::inspect::ComponentDumper;
use engine_core::scheduler::{SystemArgs, SystemError};
use engine_core::sim::Sim;
use rand::Rng;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Position {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[derive(Serialize, Deserialize)]
pub struct Velocity {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

const ENTITY_COUNT: u32 = 5;

pub fn build(seed: u64) -> Sim {
    let mut sim = Sim::new(seed, 1.0 / 60.0);
    for i in 0..ENTITY_COUNT {
        let vx = sim.rng.gen_range(-1.0..1.0);
        let vy = sim.rng.gen_range(-1.0..1.0);
        sim.world.spawn((
            Position {
                x: i as f32,
                y: 0.0,
                z: 0.0,
            },
            Velocity {
                x: vx,
                y: vy,
                z: 0.0,
            },
        ));
    }
    sim.scheduler_mut().add_system("movement", movement_system);
    sim
}

pub(crate) fn movement_system(args: &mut SystemArgs) -> Result<(), SystemError> {
    for (_e, (pos, vel)) in args.world.query::<(&mut Position, &Velocity)>().iter() {
        pos.x += vel.x * args.dt;
        pos.y += vel.y * args.dt;
        pos.z += vel.z * args.dt;
    }
    Ok(())
}

impl crate::registry::Named for Position {
    const NAME: &'static str = "Position";
}
impl crate::registry::Named for Velocity {
    const NAME: &'static str = "Velocity";
}

pub const DUMPERS: &[ComponentDumper] = &[
    crate::registry::dump::<Position>,
    crate::registry::dump::<Velocity>,
];
