# ADR-0006: Content scripting via Lua + hot-reload scope

- **Status**: accepted
- **Date**: 2026-08-31

## Context

Phase 4 (see [ROADMAP.md](../roadmap/completed-phases.md)) closes the edit-run-observe loop: editing a scene file or a Lua content script while `engine run` is active should take effect without restarting the process, and gameplay *systems* stay native Rust while Lua is scoped to low-stakes content (dialogue/quest/tuning), per ADR-0003's own forward reference. This forces several concrete decisions: where scripting logic plugs into the existing `engine-core`/`engine-scene`/`engine-cli` layering, how much of Lua's ambient capability (I/O, randomness) is safe to expose without quietly reintroducing the nondeterminism Phase 0 spent its whole budget eliminating, what "hot-reload" actually preserves across an edit, and how far to chase the roadmap's own flagged high-risk item (native Rust hot-reload).

## Decision

**1. No changes to `engine-core`.** `Sim`/`Scheduler`/`SystemArgs` are untouched. Script dispatch is a step `engine-cli`'s orchestration layer runs explicitly after `sim.step()`, operating on `sim.world` directly (already `pub`) — the same layering `Camera`/`MeshRef`/`Material` already use (owned by `engine-render`, wired into the shared registry, `engine-core` none the wiser). Baking a `Script` component or a Lua handle into `Sim` would make `engine-core` — and transitively `engine-render`/`engine-assets`, which depend on it — aware of a concrete scripting mechanism they have no business knowing about.

**2. One reload mechanism, not two.** Any watched-file change (scene *or* script) triggers a full rebuild of a fresh `Sim` from the scene file and a full rerun of the existing `--ticks` budget — there is no separate "patch a live running script in place" path. Nothing in Weft today produces runtime state worth preserving across an edit (no input system, no live windowed consumer of `engine run`); every tick is a pure function of `(seed, scene, scripts)`, so rerun-from-scratch reproduces anything an in-place patch would, with one unambiguous semantics instead of two.

**3. `--watch` reruns to completion, then blocks — no real-time pacing.** `--ticks` keeps its existing meaning exactly (a fixed budget for one run); `--watch` only changes what happens after that budget finishes: instead of exiting, the process blocks on a file watcher and reruns the same budget from scratch on the next change. Weft's audience is an agent driving the CLI headlessly, not a human watching a live view (rendering is a separate, disconnected `engine render` command) — wall-clock pacing has no consumer yet.

**4. Lua sandbox: `mlua::StdLib::ALL_SAFE`, with `math.random`/`math.randomseed` explicitly overridden to error.** `ALL_SAFE` already excludes `io`/`os`/`debug`/`ffi`, but it does include `math`, which carries ambient, unseeded randomness — exactly the kind of hole the "single seeded RNG, never ambient" rule (Phase 0, ADR-0002) exists to close, just relocated into Lua instead of Rust. No RNG binding is exposed to Lua in v1 at all; that FFI surface (how would a script's random draw interact with the engine's own seeded stream and stay replay-deterministic?) is real design work with no concrete script needing it yet. `require`/`package` are left enabled but explicitly out of hot-reload scope: v1 only watches the exact paths named by `Script` components, not anything they `require`. Documented as a known limitation rather than additionally restricted, since no content exists yet that uses `require`.

**5. The Lua↔ECS bridge reuses the existing `serde_json::Value`-based `ComponentLoader`/`ComponentDumper` registry mechanism** (ADR-0003) rather than inventing per-component Rust↔Lua FFI: a scripted entity's other components are dumped to a table via the same dumper list `engine inspect` already uses, `mlua`'s `serde` feature converts the table in and back out, and the returned value is walked back through the same named loaders that scene-file parsing already uses. This is the same "opaque passthrough" pattern ADR-0003/0005 validated twice, needing zero new generic loader mechanism.

**6. Native Rust hot-reload is not implemented.** The roadmap flags it as this phase's highest-risk item and pre-authorizes deferring it if it's a rabbit hole. `hot-lib-reloader`-style dylib reloading requires restructuring every gameplay system behind a dylib boundary and re-deriving Rust's `#[repr]`/ABI stability story across reloads — real engineering cost with no test game yet whose iteration speed it would actually improve. Deferred, not scheduled.

## Alternatives considered

- **A `ScriptHost` field on `Sim`, with `Scheduler`/`SystemArgs` gaining a generic resource slot**: rejected — breaks the engine-core/engine-cli layering split for a generality (arbitrary future "resources") nothing today needs; see Decision 1.
- **Incremental scene diff/patch on reload (match by `SceneName`, preserve unrelated entity state)**: rejected as scope disproportionate to the DoD — no runtime-mutated state exists yet that a full rebuild would lose. Revisit per "Revisit when" below.
- **Real-time-paced `--watch` (sleep to match `dt` between ticks)**: rejected — no live/windowed consumer of a real-time tick stream exists; would add timing-precision and cross-platform sleep-accuracy concerns the roadmap never asked for.
- **Binding Lua's `math.random` to the engine's seeded `EngineRng`** instead of erroring: rejected for v1 — doable, but the interleaving semantics (does a script's random draw advance the same stream native systems use? per-entity substreams?) is real design work with zero concrete scripts needing randomness yet. Revisit per "Revisit when" below.
- **Restricting `require`/`package` outright**: rejected as unneeded machinery — no authored content uses it yet; documenting the hot-reload gap is cheaper and just as honest.

## Consequences

- `engine-script` is a new crate depending on `mlua`, `hecs`, and `engine-scene` (for the registry types) — not on `engine-core` directly beyond what `engine-scene` already re-exports, keeping the dependency graph a strict DAG down from `engine-cli`.
- `ComponentRegistry::loader` (`crates/engine-scene/src/registry.rs`), previously `pub(crate)`, becomes `pub` so `engine-cli`'s dispatch orchestration can look up a loader by name for the Lua write-back step — the smallest opening that unblocks the bridge in Decision 5.
- A scene-file edit and a script edit are observably identical from the outside during `--watch` (both cause a full rerun) — simpler to reason about and test, at the cost of scripts not getting a cheaper reload path even though they could.
- Scripts cannot currently produce or consume randomness at all — acceptable for the stated Phase 4 content scope (dialogue/quest/tuning logic), a real gap for anything wanting varied behavior.

## Revisit when

- A real test game (Phase 6+) has actual runtime-mutated state (e.g. live input, accumulated player progress) that a full-rebuild reload would visibly and annoyingly discard — that's the trigger to design incremental scene reload, not before.
- A concrete script needs randomness — design the RNG-binding FFI surface then, informed by what that script actually needs (a single shared stream vs. per-entity substreams), not speculatively now.
- A test game's iteration loop is measurably bottlenecked by full-process Rust rebuilds specifically (not just Lua/scene edits, which already hot-reload) — that's the trigger to revisit native hot-reload, timeboxed again rather than assumed necessary.
