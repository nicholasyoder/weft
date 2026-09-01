use std::collections::HashSet;

/// A general-purpose keyboard key set — letters, digits, arrows, and the
/// common control/modifier keys a real control scheme needs, not just the
/// WASD+Space+Escape set the first windowed/live milestone got away with
/// (see ADR-0010). Still a plain `engine-core` enum with no dependency on
/// `winit` or any other windowing crate (see ADR-0013) — extended further
/// on demand, same scoping precedent as `ColliderShape`/`MeshKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyCode {
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Digit0,
    Digit1,
    Digit2,
    Digit3,
    Digit4,
    Digit5,
    Digit6,
    Digit7,
    Digit8,
    Digit9,
    Up,
    Down,
    Left,
    Right,
    Space,
    Enter,
    Tab,
    Escape,
    LeftShift,
    RightShift,
    LeftControl,
    RightControl,
    LeftAlt,
    RightAlt,
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
