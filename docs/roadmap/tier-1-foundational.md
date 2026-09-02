# Tier 1 — Foundational

> **This is a suggested grouping, not a queue.** "Foundational" means later tiers build on these, or retrofitting them after the fact is expensive — not that they must happen strictly first, or in the order listed below. Pull whichever item a concrete need (usually `games/sandbox`) actually points to next. See [`../../ROADMAP.md`](../../ROADMAP.md) for the full framing.

Seeded by a full capability audit of the engine (2026-09-01) against what a fully realized game — realistic graphics, audio, animation, UI, everything a shipped game needs — actually requires. The audit found the deterministic core (ECS, scenes, physics stepping, Lua dispatch, CLI/MCP) solid, but almost everything a player would actually see, hear, or feel still unbuilt. The items below are the ones judged cheapest to build now and most expensive to retrofit later.

---

## Text rendering + a minimal UI layer — **done** (2026-09-01)

`engine-render` gained a `fontdue`-based glyph atlas and a second, alpha-blended render pipeline appended to the existing single pass — no HUD/debug-overlay/menu capability existed before this (see [ADR-0014](../decisions/0014-text-rendering-and-minimal-ui.md)). Scope: screen-space `Text` only (no world-space billboards, panels, buttons, or rich text yet), but **arbitrary developer-supplied fonts work from the start**, not as a deferred follow-up — imported via the existing `engine import` command, lazily rasterized and cached per content hash exactly like imported meshes/textures already are, alongside one embedded fallback font so text renders with zero asset authoring. `games/sandbox`'s HUD ("Pickups: N/3") is the concrete proof, using a real imported custom font, not just the fallback. Per [ADR-0001](../decisions/0001-rust-3d-from-scratch.md) and the engine's own thesis, the UI layer sits strictly on top of the same CLI/API surface everything else uses (both `engine render` and `engine play` share the one `RenderContext::draw()` path) — never a capability's only entry point, satisfied automatically rather than needing a separate design pass.

## Audio playback baseline

Zero audio code exists anywhere in the workspace (no crate, no dependency, confirmed by grepping the whole repo). Start narrow: one-shot SFX playback, looping music, and a minimal mixer (master/music/SFX volume). Spatial/3D audio is real but not foundational — it's Tier 2. Needs a matching import path in `engine-assets` for audio files (wav/ogg), following the same source-agnostic, content-addressed pattern the glTF/image importer already established — and, per [ADR-0009](../decisions/0009-asset-generation-workflow.md), any *generation* tooling for audio content stays external to the engine crates, same as the existing asset-gen scripts.

## Animation data pipeline

Fully absent today, and the most structurally invasive item here: the mesh vertex format carries no bone index/weight data, so skinning has to be designed into the format before anything can be imported or played back. [ADR-0005](../decisions/0005-asset-pipeline.md) already names this a real gap, not an oversight — glTF animation channels/samplers aren't read at all currently. Doing this before Tier 2's PBR/normal-map work (which also touches the vertex format) means one vertex-format migration instead of two. Scope for this tier: the format change, glTF animation import, and basic runtime skeletal playback. Blending and state machines are Tier 3 — sequence those after basic playback actually works.

## Expanded scripting API — **done** (2026-09-01)

`engine-script`'s Lua dispatch mechanism ([ADR-0006](../decisions/0006-scripting-and-hot-reload.md)) is built and tested. A script can draw deterministic randomness (`engine.random`/`engine.random_int`), despawn itself or another entity (`engine.despawn`), see any other entity via `engine.query` (see [ADR-0012](../decisions/0012-expanded-scripting-api.md)), and read live keyboard state via `engine.key_held` (see [ADR-0013](../decisions/0013-live-script-input-and-generalized-keycode.md)) — the fourth gap ADR-0012 originally deferred, closed once `engine-cli`'s live `play` loop actually dispatched scripts to give it a live consumer. `games/sandbox` now uses scripts for real gameplay, not just batch fixtures: three pickups in `playground.toml` run `scripts/pickup.lua`, combining `engine.key_held`/`engine.query`/`engine.despawn` to let the player collect them (walk close, hold E) — see ADR-0013's follow-up note.

## Generalize keyboard input — **done** (2026-09-01)

`engine_core::KeyCode` grew from a fixed six-variant enum (W, A, S, D, Space, Escape) to a practical full keyboard set — A–Z, digits, arrows, Enter/Tab/Space/Escape, and left/right Shift/Control/Alt — see [ADR-0013](../decisions/0013-live-script-input-and-generalized-keycode.md). Mouse and gamepad support are a different kind of work (new device classes, not just more keys) and stay in Tier 4 until something concrete needs them.

---

Next: [Tier 2 — Visual & gameplay realism](tier-2-visual-and-gameplay-realism.md)
