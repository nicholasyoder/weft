# ADR-0003: TOML scene format with a caller-supplied component/system registry

- **Status**: accepted
- **Date**: 2026-08-28

## Context

Phase 1 (see [ROADMAP.md](../../ROADMAP.md)) replaces Phase 0's hardcoded-in-Rust scenarios with scenes authored as plain text, per the "text, diffable, git-mergeable" thesis in [research/03, §1](../../research/03-design-principles-for-agent-native-engines.md#1-scene-and-asset-data-formats-text-diffable-git-mergeable). Two forks in the road needed a decision: the concrete text format, and how a generic scene loader can spawn game-specific component types (`Position`, `Velocity`, ...) it has no compile-time knowledge of.

## Decision

**1. TOML, not a hand-rolled DSL**, per the ROADMAP's own default. One `[[entity]]` array-of-tables block per entity, in file order; entity identity is a human-chosen `name: String`, never an engine-recomputed counter or GUID. Component data lives in nested `[entity.components.<Name>]` tables. Systems are referenced by name only (`[[system]] name = "..."`) — Phase 1 does not put system *logic* in text, only which native systems (compiled Rust, registered in advance) a scene wires up. Scripted logic is explicitly Phase 4's job (Lua via `mlua`), not this phase's.

**2. A caller-supplied `ComponentRegistry`/`SystemRegistry`, not a derive macro or reflection.** `engine-scene` (`crates/engine-scene`) knows the TOML shape and the load *mechanism* but nothing about `Position`/`Velocity` or `movement_system` — those stay defined once in `engine-cli` (`crates/engine-cli/src/scenarios/basic.rs`), which builds a registry (`crates/engine-cli/src/registry.rs`) mapping component/system name strings to that same code. This mirrors the type-erasure-via-fn-pointer pattern `engine_core::inspect::ComponentDumper` already established in Phase 0 (a `fn(&EntityRef) -> Option<(&str, Value)>` erases the concrete component type on the way out; `ComponentLoader` is the same shape in reverse, `fn(Value, &mut EntityBuilder) -> Result<(), Error>`, erasing it on the way in).

**3. Every scene-loaded entity gets an engine-scene-owned `SceneName` component automatically**, dumped as a `"SceneName"` JSON field independent of the caller's registry. `inspect::world_to_json` already sorts by hecs's internal `Entity` id for stable output, but that id is an opaque, spawn-order-derived debug string (`"0v1"`) — not something a human or agent diffing two JSON dumps can use to say "this is the same entity as before." `SceneName` gives every scene-file entity a diff-stable anchor that traces directly back to the line the author wrote.

## Alternatives considered

- **Reflection/derive-macro-based component registration** (e.g. a `#[derive(SceneComponent)]` that auto-registers a type): rejected as premature machinery. With exactly one consumer (`engine-cli`) and two component types, a hand-written registry is a few lines; a macro would be speculative infrastructure for a need that doesn't exist yet (no `games/sandbox` crate, no second registrant).
- **Scaffolding `games/sandbox` now to hold `Position`/`Velocity`**: rejected for this phase — ROADMAP already defers that crate until real content forces its shape, and reusing `scenarios/basic.rs`'s existing types (made `pub(crate)`) keeps Phase 1's diff focused on the scene-loading mechanism itself.
- **Storing `SceneName` as a caller-registered component** instead of engine-scene's own built-in: would make the diff-anchor optional per-game and inconsistent; making it unconditional costs nothing (one more `hecs::EntityBuilder::add` per spawn) and guarantees every scene file gets diffable output for free.

## Consequences

- Adding a new component or system to a scene file requires a matching Rust-side registration in whatever crate owns the registry (today, `engine-cli::registry`) — there's no way to invent a component purely in TOML. This is intentional: components remain typed Rust, text only supplies data.
- `SimSource` (the `Scenario(String) | Scene(PathBuf)` enum in `engine-cli::lib`) is the one place `test`/`inspect`/`run`/`replay` all resolve "what am I running," so scene-file support reached every existing command instead of only the new `run` subcommand.

## Revisit when

- If a second registrant of `ComponentRegistry`/`SystemRegistry` appears (e.g. `games/sandbox` gets scaffolded, or `engine-mcp` needs its own), re-evaluate whether the registry should move to a shared crate or grow a derive macro to cut boilerplate.
- If scene files need to reference *other* entities by name (e.g. a parent/child or trigger-target relationship), `SceneName` as a plain component is not enough — that would need a name→`Entity` resolution pass after spawning, which doesn't exist yet.
