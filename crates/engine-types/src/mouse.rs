/// Raw relative mouse motion accumulated since the last live-loop frame
/// snapshot. Unlike `Input.held` (persists across frames — see its doc
/// comment), this is a per-frame *delta*: `live::App` must reset it to
/// zero immediately after inserting each frame's snapshot into
/// `Resources`, or the same motion would be re-applied on every
/// accumulator-loop tick that frame (matching `Input`'s own "one snapshot
/// reused for however many ticks" behavior, not compounding on top of it).
/// Never inserted by any batch command (`test`/`run`/`render`/`replay`/
/// `mix`) — only `live::play` has a real mouse, so a consuming system
/// must treat an absent `MouseDelta` as "no motion," exactly like
/// `player_control_system` already treats an absent `Input`.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct MouseDelta {
    pub dx: f32,
    pub dy: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_zero() {
        let delta = MouseDelta::default();
        assert_eq!(delta, MouseDelta { dx: 0.0, dy: 0.0 });
    }
}
