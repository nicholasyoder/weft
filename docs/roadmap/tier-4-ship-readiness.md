# Tier 4 — Ship readiness

> **This is a suggested grouping, not a queue.** Pull whichever item a concrete need actually points to next, from any tier — don't treat tier order as a commitment. See [`../../ROADMAP.md`](../../ROADMAP.md) for the full framing.

The lowest urgency of the four tiers — nothing here matters until an actual game is close enough to finished that shipping it, on more than one machine or input device, becomes a real question.

---

## Packaging & platform

- **Export/packaging pipeline** — no standalone-executable build or asset bundling exists yet; running a game today means `cargo run -p <game>` from source.
- **Cross-platform verification** — `winit` supports Windows and macOS in principle, but neither has actually been run; only Linux (via `Xvfb` in the dev sandbox) has ever been verified.
- **WASM / console / mobile targets** — not attempted.
- **CI build matrix** — no automated cross-platform build verification exists yet.

## Additional input devices

- **Mouse input** — no cursor position, click, or scroll handling exists.
- **Gamepad / controller input** — not wired up.
- **Touch input** — not wired up, and only worth building alongside the mobile-platform work above, not speculatively ahead of it.

## Networking / multiplayer

No network crate exists anywhere in the workspace today. Stays the lowest-priority item on this entire roadmap — only worth picking up once a multiplayer game is actually planned. Worth remembering when that day comes: `rapier3d`'s determinism is reliable on one machine/build but not guaranteed bit-identical across different hardware, a real constraint for lockstep-style networking (already flagged in the engine's history, back when physics first landed).

---

Previous: [Tier 3 — Polish & feel](tier-3-polish-and-feel.md) · Back to [`../../ROADMAP.md`](../../ROADMAP.md)
