# ADR-0012: Expanded Lua scripting API — RNG, despawn, entity queries

- **Status**: accepted
- **Date**: 2026-09-01

## Context

Phase 9 ([ADR-0011](0011-entity-despawn-eviction.md)) closed the "no entity is ever despawned" gap and explicitly deferred its scripting angle. The Tier 1 roadmap's "Expanded scripting API" item ([docs/roadmap/tier-1-foundational.md](../roadmap/tier-1-foundational.md)) named four concrete gaps in `engine-script`'s Lua bridge: a script can only read/write its own entity's components, with no RNG access, no despawn access, no way to see any other entity, and no input access. `games/sandbox` used zero scripts as a result — every actual gameplay system was native Rust, which undercuts Weft's own "agent-authorable via text/scripts" thesis more directly than any other unbuilt Tier 1 item.

## Decision

**Scope: RNG, despawn, and entity queries are implemented here. Input access is explicitly deferred**, not scoped down for size reasons but because of a real architectural gap this work surfaced: `engine-cli`'s live `play` loop (`games/sandbox`'s run path) never calls `ScriptHost::dispatch` at all — `Script`-tagged entities are only ever dispatched by `run`/`test`/`inspect`/`replay`/`watch`. Wiring live input into scripts means first deciding whether/how `play` should dispatch scripts at all (timing semantics, whether a script should see input every frame or every fixed tick, etc.) — a separate design question with no concrete script demanding it yet. RNG/despawn/query are proven the same way Phase 9 proved its own mechanism: a new scene fixture (`tests/fixtures/scenes/scripted_gameplay.toml`), not `games/sandbox`, which is blocked on the play-dispatch gap regardless of what this ADR builds.

**1. All three additions are exposed as an `engine` Lua table, rebuilt fresh per `dispatch_one` call via `mlua::Lua::scope`.** The bound functions close over `ctx.world`/`ctx.rng`, which only live for the duration of one `dispatch()` call, not the whole `ScriptHost` — `Lua::scope` is mlua's mechanism for binding non-`'static` closures for exactly this reason. Several closures need to share access to the same `&mut World`/`&mut EngineRng`; a `RefCell`/`Cell` per shared value lets them do so safely (only one is ever live at a time, since Lua calls them synchronously, never concurrently) without each closure needing to independently own a `&mut` borrow, which the borrow checker wouldn't allow for multiple simultaneously-alive closures anyway.

**2. `engine.random(lo, hi)` / `engine.random_int(lo, hi)`, a single shared stream, not per-entity substreams.** Both draw from the same `EngineRng` the whole `Sim` already uses (threaded in via a new `DispatchCtx::rng: &mut EngineRng` field, set by `engine-cli`'s `step_and_dispatch` from `&mut sim.rng`). Draws happen in the same sorted-entity-id order `dispatch` already establishes (ADR-0002), so the full per-tick draw order is: native systems in scheduler order, then scripted entities in `to_bits()` order — deterministic and replayable with no new mechanism. `math.random`/`math.randomseed` stay disabled exactly as ADR-0006 left them; `engine.random*` is a separate, explicit namespace, not a resurrection of Lua's ambient RNG.

**3. `engine.despawn()` / `engine.despawn(id)`.** With no argument, despawns the entity currently being dispatched; the write-back step (`ctx.world.insert`) is then skipped even if the script still returns a component table, tracked via a `Cell<bool>`. With an id argument, despawns an arbitrary entity, returning `true`/`false` for found/not-found rather than erroring — a despawn target already gone isn't exceptional in a sorted-snapshot dispatch loop (the same posture `despawn_after_system` already takes with `let _ = args.world.despawn(entity)`).

**4. `engine.query({"ComponentA", ...})`**, returning an array of `{ id = <number>, ComponentA = {...}, ... }` for every live entity that has *all* requested component names — reusing a new shared `dump_entity` helper (factored out of the self-input-table code `dispatch_one` already had) against `ctx.dumpers`, the same list already threaded through `DispatchCtx`. No `ComponentRegistry` changes needed for read-only queries.

**5. Entity ids cross the Lua boundary as `Entity::to_bits(): NonZeroU64` round-tripped through an f64.** Safe up to 2^53 — far past any realistic entity count here — called out as a deliberate scope limit, not a silent risk.

**6. A script's own entity id is passed as a fourth, trailing call argument**: `function(components, tick, dt, self_id)`. Trailing (not inserted earlier) so the two pre-existing fixture scripts (`counter.lua`, `increment_x.lua`, both `function(components, tick, dt)`) stay valid unchanged — Lua ignores extra call arguments a function doesn't declare.

## A gap this work surfaced, not fixed here

Building the first scene with *two* different script files loaded into one `ScriptHost` (`tests/fixtures/scenes/scripted_gameplay.toml`, whose `collector.lua` and `fuse.lua` both originally used `function on_tick(...)`) hit a real, pre-existing limitation: `ScriptHost::load_file` defines every loaded chunk's top-level functions as *globals*, so two scripts that happen to name their function the same collide — the second-loaded one silently shadows the first, and the wrong function runs for the first entity. Every prior fixture only ever had one `Script`-tagged entity active in a `ScriptHost` at once, so this never surfaced. Routed around here by giving each fixture script a distinct function name (`collect`, `tick_fuse`) rather than fixed — a real content-authoring footgun for any scene with more than one script, worth fixing (e.g. per-script sandboxed environments via `mlua`'s `set_environment`) before scenes commonly have several scripts, but out of scope for this pass since no test content needs more than the two-script case that just proved the bug exists.

## Alternatives considered

- **Extending the declarative return-table schema instead of imperative `engine.*` functions** (e.g. a script returns `{ despawn = true, query = {...} }` and the host interprets it after the call returns). Rejected: `query` needs to hand data *back into* the running script to influence its own logic before it returns (e.g. "despawn only pickups within range"), which a single after-the-fact return value can't express without inventing a much larger structured-command language. Imperative host-bound functions, called mid-script, are the standard `mlua` pattern for exactly this and needed no protocol redesign.
- **Per-entity RNG substreams** (e.g. seeded from `(seed, entity, tick)`). Rejected for now per ADR-0006's own "Revisit when": no concrete script needs independent, entity-local randomness yet; a single shared stream is simpler and still fully deterministic. Revisit if a script ever needs its random draws to be unaffected by how many *other* scripted entities ran before it that tick.
- **A despawn event/hook mechanism**, generalizing ADR-0011's explicitly-deferred idea. Still rejected for the same reason ADR-0011 gave: no second consumer exists yet beyond `PhysicsState`'s reactive eviction scan, which already handles a script-triggered despawn with zero changes (it evicts any despawned entity's body reactively, regardless of *what* despawned it).
- **Wiring script dispatch into `play` now, to unblock input access in the same pass.** Rejected as a real scope expansion with its own design questions (per-frame vs per-tick dispatch timing, whether `games/sandbox`'s existing native systems and a script could both react to input in the same tick) that no concrete script currently forces an answer to. Left as the explicit remaining piece of the roadmap item.

## Consequences

- `engine-script` now depends on `rand` directly (for `Rng::gen_range` on the shared `EngineRng`), not just `rand_chacha` transitively through `engine-core`.
- `DispatchCtx` gained a third lifetime parameter (`rng: &'r mut EngineRng`) alongside `world`/`components`+`dumpers`; `step_and_dispatch` in `engine-cli` passes `&mut sim.rng` through.
- A script that stashes a reference to `engine.despawn`/`engine.query`/`engine.random*` in a Lua variable and calls it outside the tick it was handed in will get an `mlua` scope-invalidation error — an inherent `Lua::scope` limitation, not Weft-specific, and not additionally guarded against since no content does this.
- Multi-script scenes must give each script's function a globally-unique name until the global-namespace collision noted above is fixed.

## Revisit when

- A concrete script needs live input — that's the trigger to design `play`'s script-dispatch integration, not before.
- A scene commonly has more than a couple of scripts and the shared-global-function-namespace collision becomes a routine authoring footgun rather than a one-off worked around by naming discipline.
- A script's randomness needs to be independent of dispatch order (e.g. adding or removing an unrelated scripted entity shouldn't change another script's random draws) — design per-entity RNG substreams then.
