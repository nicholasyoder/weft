use std::any::{Any, TypeId};
use std::collections::HashMap;

/// A type-erased bag of per-`Sim` state that persists across ticks, for
/// subsystems (e.g. physics) whose state can't live as ordinary ECS
/// components and doesn't fit the `world`/`rng` special-cased fields on
/// `Sim`. Keyed by concrete type, never iterated, so it carries no
/// iteration-order determinism concerns (see ADR-0008).
#[derive(Default)]
pub struct Resources {
    map: HashMap<TypeId, Box<dyn Any>>,
}

impl Resources {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert<T: 'static>(&mut self, value: T) {
        self.map.insert(TypeId::of::<T>(), Box::new(value));
    }

    pub fn get<T: 'static>(&self) -> Option<&T> {
        self.map
            .get(&TypeId::of::<T>())
            .and_then(|b| b.downcast_ref::<T>())
    }

    pub fn get_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.map
            .get_mut(&TypeId::of::<T>())
            .and_then(|b| b.downcast_mut::<T>())
    }

    pub fn get_or_insert_with<T: 'static>(&mut self, default: impl FnOnce() -> T) -> &mut T {
        self.map
            .entry(TypeId::of::<T>())
            .or_insert_with(|| Box::new(default()))
            .downcast_mut::<T>()
            .expect("TypeId-keyed entry downcasts to its own type")
    }

    pub fn remove<T: 'static>(&mut self) -> Option<T> {
        self.map
            .remove(&TypeId::of::<T>())
            .and_then(|b| b.downcast::<T>().ok())
            .map(|b| *b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    struct Counter(u32);

    struct Other(&'static str);

    #[test]
    fn get_on_empty_resources_returns_none() {
        let resources = Resources::new();
        assert!(resources.get::<Counter>().is_none());
    }

    #[test]
    fn insert_then_get_returns_the_value() {
        let mut resources = Resources::new();
        resources.insert(Counter(1));
        assert_eq!(resources.get::<Counter>(), Some(&Counter(1)));
    }

    #[test]
    fn get_mut_allows_in_place_mutation() {
        let mut resources = Resources::new();
        resources.insert(Counter(1));
        resources.get_mut::<Counter>().unwrap().0 += 41;
        assert_eq!(resources.get::<Counter>(), Some(&Counter(42)));
    }

    #[test]
    fn distinct_types_do_not_collide() {
        let mut resources = Resources::new();
        resources.insert(Counter(7));
        resources.insert(Other("hello"));
        assert_eq!(resources.get::<Counter>(), Some(&Counter(7)));
        assert_eq!(resources.get::<Other>().unwrap().0, "hello");
    }

    #[test]
    fn get_or_insert_with_only_runs_default_once() {
        let mut resources = Resources::new();
        let mut calls = 0;
        resources.get_or_insert_with(|| {
            calls += 1;
            Counter(1)
        });
        resources
            .get_or_insert_with(|| {
                calls += 1;
                Counter(999)
            })
            .0 += 1;
        assert_eq!(calls, 1);
        assert_eq!(resources.get::<Counter>(), Some(&Counter(2)));
    }

    #[test]
    fn remove_returns_the_value_and_clears_the_slot() {
        let mut resources = Resources::new();
        resources.insert(Counter(5));
        assert_eq!(resources.remove::<Counter>(), Some(Counter(5)));
        assert!(resources.get::<Counter>().is_none());
    }

    #[test]
    fn remove_on_absent_type_returns_none() {
        let mut resources = Resources::new();
        assert_eq!(resources.remove::<Counter>(), None);
    }
}
