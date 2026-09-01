# Tier 1 — Foundational

> **This is a suggested grouping, not a queue.** "Foundational" means later tiers build on these, or retrofitting them after the fact is expensive — not that they must happen strictly first, or in the order listed below. Pull whichever item a concrete need (usually `games/sandbox`) actually points to next. See [`../../ROADMAP.md`](../../ROADMAP.md) for the full framing.

Seeded by a full capability audit of the engine (2026-09-01) against what a fully realized game — realistic graphics, audio, animation, UI, everything a shipped game needs — actually requires. The audit found the deterministic core (ECS, scenes, physics stepping, Lua dispatch, CLI/MCP) solid, but almost everything a player would actually see, hear, or feel still unbuilt. The items below are the ones judged cheapest to build now and most expensive to retrofit later.

---

## Text rendering + a minimal UI layer

`engine-render` has no glyph/font pipeline at all today — no HUD, menu, dialogue box, or even a debug overlay is possible without one. Worth building before the rendering pipeline picks up shadows, PBR, and post-processing on top of it (Tier 2/3), while there's less to integrate against. Per [ADR-0001](../decisions/0001-rust-3d-from-scratch.md) and the engine's own thesis, any UI layer has to sit strictly on top of the same CLI/API surface everything else uses — never a capability's only entry point.

## Audio playback baseline

Zero audio code exists anywhere in the workspace (no crate, no dependency, confirmed by grepping the whole repo). Start narrow: one-shot SFX playback, looping music, and a minimal mixer (master/music/SFX volume). Spatial/3D audio is real but not foundational — it's Tier 2. Needs a matching import path in `engine-assets` for audio files (wav/ogg), following the same source-agnostic, content-addressed pattern the glTF/image importer already established — and, per [ADR-0009](../decisions/0009-asset-generation-workflow.md), any *generation* tooling for audio content stays external to the engine crates, same as the existing asset-gen scripts.

## Animation data pipeline

Fully absent today, and the most structurally invasive item here: the mesh vertex format carries no bone index/weight data, so skinning has to be designed into the format before anything can be imported or played back. [ADR-0005](../decisions/0005-asset-pipeline.md) already names this a real gap, not an oversight — glTF animation channels/samplers aren't read at all currently. Doing this before Tier 2's PBR/normal-map work (which also touches the vertex format) means one vertex-format migration instead of two. Scope for this tier: the format change, glTF animation import, and basic runtime skeletal playback. Blending and state machines are Tier 3 — sequence those after basic playback actually works.

## Expanded scripting API

`engine-script`'s Lua dispatch mechanism ([ADR-0006](../decisions/0006-scripting-and-hot-reload.md)) is built and tested, but `games/sandbox` uses zero scripts today — every system in the actual game is native Rust. A script can currently only read and write its own entity's components. Missing: a deterministic RNG binding (deferred per ADR-0006, still no binding at all), entity spawn/despawn (the engine-level mechanism now exists per [ADR-0011](../decisions/0011-entity-despawn-eviction.md) — no Lua binding for it yet), generic entity queries (a script can't see any entity but itself), and input access. Closing this gap is what turns scripting from a proven-but-unused mechanism into something real gameplay logic can actually use.

## Generalize keyboard input

`engine_core::KeyCode` is a fixed six-variant enum (W, A, S, D, Space, Escape) — fine for a physics playground, not enough for a real control scheme. Broadening this to a general key map is cheap and unblocks most next gameplay milestones. Mouse and gamepad support are a different kind of work (new device classes, not just more keys) and stay in Tier 4 until something concrete needs them.

---

Next: [Tier 2 — Visual & gameplay realism](tier-2-visual-and-gameplay-realism.md)
