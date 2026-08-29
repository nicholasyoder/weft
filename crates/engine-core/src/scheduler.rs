use crate::rng::EngineRng;

pub struct SystemArgs<'a> {
    pub world: &'a mut hecs::World,
    pub rng: &'a mut EngineRng,
    pub tick: u64,
    pub dt: f32,
}

pub type System = fn(&mut SystemArgs);

#[derive(Default)]
pub struct Scheduler {
    systems: Vec<(String, System)>,
}

impl Scheduler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_system(&mut self, name: impl Into<String>, f: System) -> &mut Self {
        self.systems.push((name.into(), f));
        self
    }

    pub fn tick(&mut self, world: &mut hecs::World, rng: &mut EngineRng, tick: u64, dt: f32) {
        for (_name, sys) in &self.systems {
            let mut args = SystemArgs {
                world,
                rng,
                tick,
                dt,
            };
            sys(&mut args);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng;

    struct Marker(f32);

    // Order-dependent on purpose: "set to 1" then "double" only yields 2.0
    // if registration order (set, double) is honored by `tick`.
    fn set_to_one(args: &mut SystemArgs) {
        for (_e, m) in args.world.query::<&mut Marker>().iter() {
            m.0 = 1.0;
        }
    }
    fn double(args: &mut SystemArgs) {
        for (_e, m) in args.world.query::<&mut Marker>().iter() {
            m.0 *= 2.0;
        }
    }
    fn noop_system(_args: &mut SystemArgs) {}

    #[test]
    fn tick_runs_registered_systems_in_registration_order() {
        let mut world = hecs::World::new();
        let entity = world.spawn((Marker(0.0),));
        let mut rng = rng::seeded(0);
        let mut scheduler = Scheduler::new();
        scheduler
            .add_system("set_to_one", set_to_one)
            .add_system("double", double);
        scheduler.tick(&mut world, &mut rng, 0, 1.0 / 60.0);

        assert_eq!(world.get::<&Marker>(entity).unwrap().0, 2.0);
    }

    #[test]
    fn tick_with_noop_system_does_not_panic() {
        let mut world = hecs::World::new();
        let mut rng = rng::seeded(0);
        let mut scheduler = Scheduler::new();
        scheduler.add_system("noop", noop_system);
        scheduler.tick(&mut world, &mut rng, 5, 1.0 / 60.0);
    }
}
