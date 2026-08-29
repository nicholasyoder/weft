use engine_core::inspect::ComponentDumper;
use engine_core::scheduler::System;

/// Deserializes one named component's JSON representation (converted from
/// its TOML table in the scene file) and adds it to the entity under
/// construction. Mirrors `ComponentDumper` but in the opposite direction.
pub type ComponentLoader =
    fn(serde_json::Value, &mut hecs::EntityBuilder) -> Result<(), serde_json::Error>;

#[derive(Default)]
pub struct ComponentRegistry {
    entries: Vec<(&'static str, ComponentLoader, ComponentDumper)>,
}

impl ComponentRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(
        &mut self,
        name: &'static str,
        loader: ComponentLoader,
        dumper: ComponentDumper,
    ) -> &mut Self {
        self.entries.push((name, loader, dumper));
        self
    }

    pub(crate) fn loader(&self, name: &str) -> Option<ComponentLoader> {
        self.entries
            .iter()
            .find(|(n, _, _)| *n == name)
            .map(|(_, l, _)| *l)
    }

    pub(crate) fn dumpers(&self) -> Vec<ComponentDumper> {
        self.entries.iter().map(|(_, _, d)| *d).collect()
    }
}

#[derive(Default)]
pub struct SystemRegistry {
    entries: Vec<(&'static str, System)>,
}

impl SystemRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, name: &'static str, system: System) -> &mut Self {
        self.entries.push((name, system));
        self
    }

    pub(crate) fn find(&self, name: &str) -> Option<System> {
        self.entries
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, s)| *s)
    }
}
