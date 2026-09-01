# ADR-0001: Rust, 3D-from-day-one, and building from scratch

- **Status**: accepted
- **Date**: 2026-08-28

## Context

[research/00-synthesis-and-recommendations.md](../../research/00-synthesis-and-recommendations.md) flagged three open decisions before writing any code: primary language, 2D vs. 3D scope for a first slice, and whether to build a bespoke engine vs. prototype the agent-facing contract on top of an already-headless, already-text-based substrate (e.g. `bevy_ecs` directly, or Godot without its editor).

## Decision

1. **Language: Rust.** Matches the compiler-diagnostics-as-self-correction argument in [research/03](../../research/03-design-principles-for-agent-native-engines.md#6-scriptingextension-layer), and gives access to the existing agent-friendly ECS/rendering crate ecosystem (`hecs`/`flecs`, `wgpu`, `rapier3d`) without requiring a full engine dependency.
2. **3D from day one**, not 2D-first. Rationale given: avoid a rework/migration partway through once 3D is added — the renderer, scene format, and asset pipeline are designed for 3D from the start even though early milestones may only exercise trivial 3D scenes (a cube, a camera).
3. **Build from scratch**, not wrapping Godot/Bevy/etc. Focused libraries (wgpu, an ECS crate, rapier3d, mlua) are fine and expected — the constraint is specifically on not inheriting an existing *engine's* architecture, app-model, or editor assumptions. This is a deliberate rejection of the "wrap an existing agent-friendly-enough substrate" alternative that Summer Engine and OpenGame both chose (see [research/01, §2](../../research/01-prior-art-engines-tooling.md#2-projects-explicitly-building-game-enginestoolchains-for-ai-agents)) — chosen explicitly to retain full control over the architecture rather than trade speed-to-first-loop for that control.

## Alternatives considered

- **Wrap Godot headless + MCP bridge**: fastest path to a working agent loop (Summer Engine's proven approach), but inherits Godot's node/scene-tree model, GDScript conventions, and long-term roadmap constraints. Rejected per explicit user preference for architectural control.
- **Build on `bevy_ecs` + Bevy's broader ecosystem**: keeps ECS work-from-scratch small, but risks pulling in Bevy's App/Plugin/reflection conventions and release cadence. Left as a possible fallback if a hand-rolled ECS integration (ADR-0002) proves to be a time sink.
- **2D-first**: would validate the CLI/inspect/replay/MCP loop faster (every fast-shipping prior-art example — PICO-8, LÖVE, Rosebud, OpenGame — started 2D). Rejected to avoid a later rework once 3D lands.

## Consequences

- Slower time-to-first-working-loop than wrapping an existing engine, in exchange for no inherited architectural constraints.
- Early milestones (Phase 0–2 in [ROADMAP.md](../roadmap/completed-phases.md)) will front-load renderer and scene-format work that a 2D-first or wrap-based approach could have deferred or skipped.
- Full ownership of the text scene format, ECS integration, and rendering pipeline — nothing here is dictated by an upstream engine's release cycle or design opinions.

## Revisit when

- If Phase 0–2 (core loop + headless 3D rendering) is taking dramatically longer than expected and a wrapped-substrate prototype would clearly de-risk the agent-tooling design faster — consider a throwaway Godot/`bevy_ecs`-based spike purely to validate the CLI/MCP/inspect contract, without abandoning the from-scratch engine itself.
- If Rust's ecosystem turns out to lack a viable crate for a specific need (e.g. mature deterministic physics) badly enough to threaten the timeline.
