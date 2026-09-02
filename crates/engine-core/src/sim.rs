use crate::resources::Resources;
use crate::rng::{self, EngineRng};
use crate::scheduler::{Scheduler, SystemError};

pub struct Sim {
    pub world: hecs::World,
    pub rng: EngineRng,
    pub resources: Resources,
    pub tick: u64,
    pub dt: f32,
    scheduler: Scheduler,
}

impl Sim {
    pub fn new(seed: u64, dt: f32) -> Self {
        Self {
            world: hecs::World::new(),
            rng: rng::seeded(seed),
            resources: Resources::new(),
            tick: 0,
            dt,
            scheduler: Scheduler::new(),
        }
    }

    pub fn scheduler_mut(&mut self) -> &mut Scheduler {
        &mut self.scheduler
    }

    /// Advances the sim by one tick. Only increments `self.tick` on success
    /// — a tick that failed partway through a system didn't complete, so
    /// its number is not consumed. Returns the failing system's name
    /// alongside its `SystemError` on failure (see `Scheduler::tick`).
    pub fn step(&mut self) -> Result<(), (String, SystemError)> {
        self.scheduler.tick(
            &mut self.world,
            &mut self.rng,
            &mut self.resources,
            self.tick,
            self.dt,
        )?;
        self.tick += 1;
        Ok(())
    }

    pub fn run(&mut self, ticks: u64) -> Result<(), (String, SystemError)> {
        for _ in 0..ticks {
            self.step()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_advances_tick_counter_by_exactly_n() {
        let mut sim = Sim::new(0, 1.0 / 60.0);
        sim.run(10).unwrap();
        assert_eq!(sim.tick, 10);
    }

    #[test]
    fn step_does_not_advance_tick_on_a_failing_system() {
        let mut sim = Sim::new(0, 1.0 / 60.0);
        fn failing(_args: &mut crate::scheduler::SystemArgs) -> Result<(), SystemError> {
            Err(SystemError::new("TEST_FAILED", "intentional"))
        }
        sim.scheduler_mut().add_system("failing", failing);
        let err = sim.step().unwrap_err();
        assert_eq!(err.0, "failing");
        assert_eq!(sim.tick, 0);
    }
}
