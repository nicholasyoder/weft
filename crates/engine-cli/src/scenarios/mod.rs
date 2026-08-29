pub mod basic;
pub mod broken_rng;

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
];

pub fn find(name: &str) -> Option<&'static Scenario> {
    SCENARIOS.iter().find(|s| s.name == name)
}

pub fn names() -> Vec<&'static str> {
    SCENARIOS.iter().map(|s| s.name).collect()
}
