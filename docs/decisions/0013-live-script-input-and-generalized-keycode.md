# ADR-0013: Live script input access, and a generalized `KeyCode`

- **Status**: accepted
- **Date**: 2026-09-01

## Context

ADR-0012 named input access as the fourth gap in `engine-script`'s Lua bridge and explicitly deferred it: `engine-cli`'s live `play` loop (`games/sandbox`'s run path, `crates/engine-cli/src/live.rs`) never called `ScriptHost::dispatch` at all — every batch command (`run`/`test`/`inspect`/`replay`) dispatches scripts, `play` never did. Closing that gap forces two coupled design questions: when should `play` dispatch scripts relative to its own fixed-timestep loop, and how should a dispatched script actually read input? A separate, adjacent Tier 1 roadmap item — "generalize keyboard input" — was sitting right next to it: `engine_core::KeyCode` was a hardcoded 6-variant enum (W/A/S/D/Space/Escape), scoped exactly that narrow by ADR-0010 as "extended on demand." Giving scripts input access while `KeyCode` only covers WASD would just move the narrowness into Lua; generalizing `KeyCode` was only worth doing now because this work is what forced `play`'s dispatch wiring in the first place. Closing both together finishes Tier 1's last half-open item.

## Decision

**Dispatch timing**: `play` dispatches scripts once per fixed tick, immediately after `Sim::step()`, inside the accumulator loop — exactly the same step-then-dispatch order every batch command already uses. No new timing model: `live.rs` now calls a new `engine_cli::step_and_dispatch_with_input` (a thin wrapper around the existing `step_and_dispatch`, taking a live `Input` instead of always defaulting to an empty one) rather than duplicating the step/dispatch/first-error-bails logic `step_and_dispatch` already had.

**Native systems vs. scripts reacting to input in the same tick**: no new ordering rule needed. Both are read-only consumers of `Input` this tick — native systems read it via `Resources` inside `sim.step()` (unchanged since ADR-0010), scripts read it via the new `engine.key_held(name)` Lua function. A script dispatched right after `sim.step()` already sees that tick's native-system effects (same as the existing despawn/component-write interaction ADR-0012 established); adding a second read-only `Input` consumer doesn't change that.

**Threading `Input` into scripts**: `engine_script::DispatchCtx` gained a fifth field, `input: &'i Input` (a new lifetime, alongside `world`/`components`/`dumpers`/`rng`). Batch commands have no live input source, so `step_and_dispatch` (still the function `run`/`test`/`inspect`/`replay` call) forwards a fixed `&Input::default()` — nothing held, a deterministic constant, not undefined state. `live::play`'s `App` now owns its `dumpers`/`host`/`components` for its whole run (previously `dumpers` was discarded and `components` only used once, for the initial `engine_scene::load` call) and threads its real, live `self.input` through every tick.

**`engine.key_held(name)`**: one string-keyed lookup function, bound in `dispatch_one`'s existing `lua.scope` block alongside `random`/`despawn`/`query` — the same read-only-closure-over-`ctx`-field pattern `dumpers` already used for `query`, needing no `Cell`/`RefCell` since `Input` is read-only here. An unrecognized name is a Lua runtime error (`engine.key_held: '<name>' is not a recognized key name`), the same "surface a scripting typo immediately" posture `engine.random`/`engine.random_int` already use for invalid ranges — not a silent `false`. Key names match `KeyCode`'s Rust variant names exactly (`"W"`, `"Up"`, `"LeftShift"`), the same casing convention scene files already use for component names.

**`KeyCode` generalization**: expanded from 6 variants to a practical full keyboard set — all letters (A–Z), digits (`Digit0`–`Digit9`), arrows (`Up`/`Down`/`Left`/`Right`), `Enter`, `Tab`, `Space`, `Escape`, and left/right `Shift`/`Control`/`Alt`. Still a plain `engine-core` enum with **no dependency on `winit` or any windowing crate** — ADR-0010 already considered and rejected a shared input crate / winit-wrapping approach as premature for the 6-variant version, and nothing about broadening the variant list changes that reasoning; `crates/engine-cli/src/live.rs`'s `map_key` gained one matching arm per new variant, mapping against `winit::keyboard::KeyCode`'s real names (verified directly against winit 0.30's `keyboard.rs` source, not guessed).

## Alternatives considered

- **Expose individual key booleans on the `engine` table** (e.g. `engine.key_w_held()` generated per variant) instead of one `key_held(name)` function. Rejected: would need either codegen or 50+ hand-written bindings for the same information `engine.query`-style name-based lookup already gets in one function, and doesn't match the existing `engine.despawn(id)`/`engine.random(lo, hi)` precedent of parameterized functions over enumerated ones.
- **Silently return `false` for an unrecognized key name** instead of erroring. Rejected for the same reason `engine.random`'s range validation errors instead of clamping: a typo'd key name (`"Sapce"`) failing loudly during development is far cheaper than a script that silently never responds to a key no one noticed was misspelled.
- **Dispatch scripts once per rendered frame instead of once per fixed tick.** Rejected: `play`'s frame rate and `sim.dt` are already decoupled by the fixed-timestep accumulator (ADR-0010) specifically so simulation stays deterministic-per-tick regardless of real frame timing; dispatching scripts per-frame would make script-driven state depend on wall-clock frame rate, undermining the same guarantee physics/scheduler ticks already have.
- **A generic `engine-input` crate, or wrapping `winit::keyboard::KeyCode` directly.** Still rejected, per ADR-0010's original reasoning: `Input`/`KeyCode` remains small with no `winit` dependency of its own, and no second, meaningfully different input source (gamepad, mouse) exists yet to justify either move.

## Consequences

- `engine_script::DispatchCtx` now has four lifetime parameters instead of three (`'w, 'd, 'r, 'i`) and a required `input` field — every construction site (both `engine-cli` call sites, plus `engine-script`'s own crate-level tests) needed updating; batch-path callers pass a fixed `&Input::default()`.
- `engine-cli`'s `App` (the `play` event-loop handler) is now lifetime-parameterized (`App<'a>`, borrowing `components: &'a ComponentRegistry` from its caller) and owns `dumpers`/`host` for its full run, not just the initial scene load — `games/sandbox`'s `play` wrapper needed no changes, since it already passes registries that outlive the `event_loop.run_app` call.
- `games/sandbox` can now define gameplay in Lua scripts that react to live keyboard input, not just native Rust systems reading `Resources` directly — closing the gap ADR-0012 called out as undercutting Weft's own "agent-authorable via scripts" thesis. `games/sandbox` still has zero scripts as of this ADR; nothing here adds one, only unblocks it.
- A script that stashes `engine.key_held` in a variable and calls it outside the tick it was handed in hits the same inherent `mlua::Lua::scope` invalidation error ADR-0012 already documented for `despawn`/`query`/`random` — not additionally guarded against, consistent with the existing bindings.

## Revisit when

- A concrete script actually needs live input for real gameplay (not just a proof fixture) — that's the trigger to also decide whether scripts need frame-level (not just tick-level) input granularity, which nothing today demands.
- A second input device class (mouse, gamepad) becomes a real need — at that point, re-open whether `Input`/`KeyCode` staying in `engine-core` with no windowing dependency is still the right shape, per ADR-0010's own original "revisit when."
- `KeyCode`'s current set (still missing punctuation, numpad, and OS/media keys) turns out too narrow for a real control scheme — extend the same way this pass did, on demand.
