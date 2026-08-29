use crate::rng::{self, EngineRng};
use crate::scheduler::Scheduler;

pub struct Sim {
    pub world: hecs::World,
    pub rng: EngineRng,
    pub tick: u64,
    pub dt: f32,
    scheduler: Scheduler,
}

impl Sim {
    pub fn new(seed: u64, dt: f32) -> Self {
        Self {
            world: hecs::World::new(),
            rng: rng::seeded(seed),
            tick: 0,
            dt,
            scheduler: Scheduler::new(),
        }
    }

    pub fn scheduler_mut(&mut self) -> &mut Scheduler {
        &mut self.scheduler
    }

    pub fn step(&mut self) {
        self.scheduler
            .tick(&mut self.world, &mut self.rng, self.tick, self.dt);
        self.tick += 1;
    }

    pub fn run(&mut self, ticks: u64) {
        for _ in 0..ticks {
            self.step();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_advances_tick_counter_by_exactly_n() {
        let mut sim = Sim::new(0, 1.0 / 60.0);
        sim.run(10);
        assert_eq!(sim.tick, 10);
    }
}
