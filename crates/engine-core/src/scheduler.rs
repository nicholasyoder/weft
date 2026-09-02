use crate::resources::Resources;
use crate::rng::EngineRng;

pub struct SystemArgs<'a> {
    pub world: &'a mut hecs::World,
    pub rng: &'a mut EngineRng,
    pub resources: &'a mut Resources,
    pub tick: u64,
    pub dt: f32,
}

/// A system's failure, in the same `{code, message}` shape every other
/// structured error in the engine already uses (see
/// `engine-cli::diagnostics::CliError`) — added so a bad piece of
/// scene/scripted content (an unresolvable asset hash, a corrupt asset
/// file) can fail a tick loudly with a stable code instead of panicking
/// the whole process, see [ADR-0017](../../../docs/decisions/0017-system-error-channel.md).
/// A plain struct, not a per-crate `thiserror` enum: `System` is a single
/// fn-pointer type shared by every crate's systems, so the error type it
/// returns has to be one concrete type — each producing crate builds one
/// via `SystemError::new` from its own error's `.code()`/`Display`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemError {
    pub code: &'static str,
    pub message: String,
}

impl SystemError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for SystemError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for SystemError {}

pub type System = fn(&mut SystemArgs) -> Result<(), SystemError>;

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

    /// Runs every registered system in registration order, stopping at the
    /// first one that fails — the same "fail loud, no partial application"
    /// posture `engine-script`'s dispatch error handling already
    /// established, rather than silently continuing with some systems
    /// having run and others not. Returns the failing system's registered
    /// name alongside its error so callers can report which one broke.
    pub fn tick(
        &mut self,
        world: &mut hecs::World,
        rng: &mut EngineRng,
        resources: &mut Resources,
        tick: u64,
        dt: f32,
    ) -> Result<(), (String, SystemError)> {
        for (name, sys) in &self.systems {
            let mut args = SystemArgs {
                world,
                rng,
                resources,
                tick,
                dt,
            };
            sys(&mut args).map_err(|e| (name.clone(), e))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng;

    struct Marker(f32);

    // Order-dependent on purpose: "set to 1" then "double" only yields 2.0
    // if registration order (set, double) is honored by `tick`.
    fn set_to_one(args: &mut SystemArgs) -> Result<(), SystemError> {
        for (_e, m) in args.world.query::<&mut Marker>().iter() {
            m.0 = 1.0;
        }
        Ok(())
    }
    fn double(args: &mut SystemArgs) -> Result<(), SystemError> {
        for (_e, m) in args.world.query::<&mut Marker>().iter() {
            m.0 *= 2.0;
        }
        Ok(())
    }
    fn noop_system(_args: &mut SystemArgs) -> Result<(), SystemError> {
        Ok(())
    }
    fn failing_system(_args: &mut SystemArgs) -> Result<(), SystemError> {
        Err(SystemError::new(
            "TEST_SYSTEM_FAILED",
            "intentional failure",
        ))
    }

    #[test]
    fn tick_runs_registered_systems_in_registration_order() {
        let mut world = hecs::World::new();
        let entity = world.spawn((Marker(0.0),));
        let mut rng = rng::seeded(0);
        let mut resources = Resources::new();
        let mut scheduler = Scheduler::new();
        scheduler
            .add_system("set_to_one", set_to_one)
            .add_system("double", double);
        scheduler
            .tick(&mut world, &mut rng, &mut resources, 0, 1.0 / 60.0)
            .unwrap();

        assert_eq!(world.get::<&Marker>(entity).unwrap().0, 2.0);
    }

    #[test]
    fn tick_with_noop_system_does_not_panic() {
        let mut world = hecs::World::new();
        let mut rng = rng::seeded(0);
        let mut resources = Resources::new();
        let mut scheduler = Scheduler::new();
        scheduler.add_system("noop", noop_system);
        scheduler
            .tick(&mut world, &mut rng, &mut resources, 5, 1.0 / 60.0)
            .unwrap();
    }

    #[test]
    fn tick_stops_at_the_first_failing_system_and_names_it() {
        let mut world = hecs::World::new();
        let mut rng = rng::seeded(0);
        let mut resources = Resources::new();
        let mut scheduler = Scheduler::new();
        scheduler
            .add_system("failing", failing_system)
            .add_system("noop", noop_system);
        let err = scheduler
            .tick(&mut world, &mut rng, &mut resources, 0, 1.0 / 60.0)
            .unwrap_err();
        assert_eq!(err.0, "failing");
        assert_eq!(err.1.code, "TEST_SYSTEM_FAILED");
    }
}
