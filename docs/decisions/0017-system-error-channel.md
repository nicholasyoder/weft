# ADR-0017: A real error channel for systems

- **Status**: accepted
- **Date**: 2026-09-02

## Context

A full critical review of the codebase (2026-09-02, see [docs/roadmap/known-issues.md](../roadmap/known-issues.md)) found that `System = fn(&mut SystemArgs)` had no way to fail. `engine-anim`'s `animation_step` and `engine-audio`'s `audio_step` both hit real fallible operations — an `Animator.clip`/`AudioSource.clip` hash that doesn't resolve in the asset store, or resolves to corrupt bytes — and had no `Result`-returning path available, so both `panic!`ed. A single bad scene/script value could crash the whole `engine` process (`run`/`test`/`inspect`/`render`/`mix`/`play`), with no structured `{code, message}` error at all — the one failure mode this project's own diagnostics contract (see AGENTS.md's "Errors — the one contract that matters most") explicitly says should never happen.

This is a structural gap, not a per-crate oversight: any future system with a real failure mode (a physics raycast against malformed data, a future save-system I/O error) would hit the same wall.

## Decision

**1. `System` becomes `fn(&mut SystemArgs) -> Result<(), SystemError>`.** `SystemError` is a new plain struct in `engine-core::scheduler` — `{ code: &'static str, message: String }`, the same shape `engine-cli::diagnostics::CliError` already uses. It is **not** a per-crate `thiserror` enum: `System` is a single fn-pointer type shared by every crate's systems (physics, animation, audio, scene-local scenario systems, game-specific systems), so whatever it returns has to be one concrete type. Each producing crate builds a `SystemError` from its own domain error's existing `.code()`/`Display` via a small local `fn to_system_error(e: DomainError) -> SystemError` helper (can't be a `From` impl — `SystemError` and e.g. `AssetError` are both foreign to a third crate like `engine-anim`, so Rust's orphan rules block it).

**2. `Scheduler::tick` stops at the first failing system and returns `(system_name, SystemError)`.** Same "fail loud, no partial application" posture `engine-script`'s dispatch error handling already established (see ADR-0006) — a tick that fails partway through doesn't continue running the remaining systems with a half-updated world. `Sim::step`/`Sim::run` propagate this and — new — only increment `Sim::tick` on success, so a failed tick's number is never consumed (it didn't complete).

**3. `engine-cli::diagnostics::CliError` gains one new constructor, `from_system_error(system, tick, &SystemError)`**, reusing the failing system's own error code and message verbatim (not wrapped in a generic "system failed" code) with `context: {"system": ..., "tick": ...}`. Exactly two call sites needed it: `step_and_dispatch_with_input` (covers `run`/`test`/`inspect`/`replay`/`mix`/`play` — every command that already funnels through it per ADR-0007) and `render_scene`'s standalone `sim.run(ticks)` call. Every other command needed zero changes.

**4. The two real panics were fixed by reusing existing error types, not inventing new machinery.** `engine-anim`'s failures (`AssetError::NotFound`, `AssetError::SkeletonDecodeFailed`/`AnimationDecodeFailed`) were already `AssetError` variants — no new error type needed. `engine-audio`'s clip-lookup failure is the same `AssetError`; its clip-*decode* failure (`kira::sound::FromFileError`, not previously represented anywhere) got one new `AudioError::ClipDecodeFailed` variant (code `AUDIO_CLIP_DECODE_ERROR`), added to the crate's existing `AudioError` enum rather than a parallel type.

**5. Every other registered system (`physics_step`, the scenario-local `movement_system`/`despawn_after_system`/`jitter_system_ambient`/test fixtures, and `games/sandbox`'s `player_control_system`/`camera_follow_system`/`hud_system`) got a mechanical signature update — `-> Result<(), SystemError>` with a trailing `Ok(())`.** None of these have a real failure mode today; this is plumbing, not a design decision, but it's worth noting *why* every system needed touching: `System` being one shared fn-pointer type means there's no way to make it fallible for only the two crates that needed it.

## Alternatives considered

- **A per-crate generic `System<E>` (or a boxed `dyn Error`) instead of one concrete `SystemError`**: rejected — `SystemRegistry`/`Scheduler` store systems from many different crates in one homogeneous `Vec`, so the error type crossing that boundary has to be one concrete type either way; a `Box<dyn Error>` would work too but throws away the stable `.code()` string every other error type in this codebase already carries, forcing `CliError` to parse or guess a code from a boxed trait object instead of reading a field.
- **Continue running remaining systems after one fails, collecting all errors (mirroring `ScriptHost::dispatch`'s "collect every error, don't stop at the first")**: rejected — dispatch collects per-*script* errors because scripts are independent, sibling pieces of content where one failing shouldn't hide another's failure; systems are ordered and often depend on each other's output within a tick (e.g. `despawn-after` must run before `physics` — see `despawn_demo.rs`), so continuing past a failure risks running a later system against a world an earlier system left in an unknown state. Stopping is the safer default; revisit if a concrete case needs otherwise.
- **Silently skip a `SystemError` and continue the tick** (treating it like the existing "no `AssetsDir`" no-op case): rejected — that no-op is for a legitimate *absent* state (no asset store configured at all); an unresolvable hash or corrupt file is a content bug that should be visible, not swallowed, per the engine-wide "no command silently no-ops on bad input" design goal (research/03 §7, quoted in AGENTS.md).

## Consequences

- Every crate that registers a system now has a `SystemError`-returning function, whether or not it can actually fail — a small, permanent tax on adding a new system, in exchange for the failure channel existing at all should a future one need it.
- `engine-anim`/`engine-audio` no longer panic on bad content data; `cargo run -p engine-cli --bin engine -- test --scene <bad-clip-hash> --format json` now exits 1 with a structured `{"error": {"code": "ASSET_NOT_FOUND", ...}}` instead of a Rust backtrace (verified directly, not just by unit test).
- `Sim::step`/`Sim::run`'s signatures changed (now `Result`-returning) — every direct caller across the workspace (test helpers included) needed a `?`/`.unwrap()` added; none needed deeper changes since none relied on the old infallible signature for anything but happy-path chaining.
- The `HashMap<Entity, RigidBodyHandle>` iteration in `engine-physics`'s pose write-back loop (a separate, lower-severity determinism concern flagged in the same review) was **not** touched by this ADR — out of scope here. (Fixed separately, 2026-09-02: the loop now collects+sorts by entity id before iterating, matching `evict_despawned`'s existing convention.)

## Revisit when

- A second system needs to report *multiple* independent failures in one tick (not just "the first system that broke") — that's the trigger to reconsider the "collect vs. stop-at-first" choice in Decision 2, informed by what that system actually needs, not speculatively now.
- `SystemError`'s flat `{code, message}` shape stops being enough context for debugging a real failure (e.g. needing structured `context` data, not just a string) — mirror `CliError`'s `context: serde_json::Value` field onto `SystemError` then, not before a concrete case needs it.
