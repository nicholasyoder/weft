use std::collections::HashSet;

/// A deliberately narrow set of keys — covers exactly what the first
/// windowed/live milestone needs (WASD movement + escape-to-quit).
/// Extended on demand, same scoping precedent as `ColliderShape`/`MeshKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyCode {
    W,
    A,
    S,
    D,
    Space,
    Escape,
}

/// Which keys are held as of the most recent live-loop poll. Not an ECS
/// component (one per `Sim`, like `rng`/a physics resource, not per-entity)
/// and never scene-authored or `engine inspect`-dumped — input is wall-clock
/// live state, not simulation state. Lives in `Resources` (see ADR-0008)
/// exactly like `engine-physics`'s physics state, reusing that mechanism
/// with zero new plumbing.
#[derive(Debug, Default, Clone)]
pub struct Input {
    held: HashSet<KeyCode>,
}

impl Input {
    pub fn is_held(&self, key: KeyCode) -> bool {
        self.held.contains(&key)
    }

    pub fn set_held(&mut self, key: KeyCode, held: bool) {
        if held {
            self.held.insert(key);
        } else {
            self.held.remove(&key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_starts_unheld() {
        let input = Input::default();
        assert!(!input.is_held(KeyCode::W));
    }

    #[test]
    fn set_held_toggles_state() {
        let mut input = Input::default();
        input.set_held(KeyCode::W, true);
        assert!(input.is_held(KeyCode::W));
        input.set_held(KeyCode::W, false);
        assert!(!input.is_held(KeyCode::W));
    }
}
