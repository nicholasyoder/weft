pub mod basic;
pub mod broken_rng;
pub mod despawn_demo;
pub mod physics_demo;

use engine_core::inspect::ComponentDumper;
use engine_core::sim::Sim;

pub struct Scenario {
    pub name: &'static str,
    pub build: fn(u64) -> Sim,
    pub dumpers: &'static [ComponentDumper],
}

pub const SCENARIOS: &[Scenario] = &[
    Scenario {
        name: "basic",
        build: basic::build,
        dumpers: basic::DUMPERS,
    },
    Scenario {
        name: "broken-rng",
        build: broken_rng::build,
        dumpers: broken_rng::DUMPERS,
    },
    Scenario {
        name: "physics-demo",
        build: physics_demo::build,
        dumpers: physics_demo::DUMPERS,
    },
    Scenario {
        name: "despawn-demo",
        build: despawn_demo::build,
        dumpers: despawn_demo::DUMPERS,
    },
];

pub fn find(name: &str) -> Option<&'static Scenario> {
    SCENARIOS.iter().find(|s| s.name == name)
}

pub fn names() -> Vec<&'static str> {
    SCENARIOS.iter().map(|s| s.name).collect()
}
