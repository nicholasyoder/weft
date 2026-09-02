# Tier 1 — Foundational

> **This is a suggested grouping, not a queue.** "Foundational" means later tiers build on these, or retrofitting them after the fact is expensive — not that they must happen strictly first, or in the order listed below. Pull whichever item a concrete need (usually `games/sandbox`) actually points to next. See [`../../ROADMAP.md`](../../ROADMAP.md) for the full framing.

Seeded by a full capability audit of the engine (2026-09-01) against what a fully realized game — realistic graphics, audio, animation, UI, everything a shipped game needs — actually requires. The audit found the deterministic core (ECS, scenes, physics stepping, Lua dispatch, CLI/MCP) solid, but almost everything a player would actually see, hear, or feel still unbuilt. The items below are the ones judged cheapest to build now and most expensive to retrofit later.

---

## Text rendering + a minimal UI layer — **done** (2026-09-01)

`engine-render` gained a `fontdue`-based glyph atlas and a second, alpha-blended render pipeline appended to the existing single pass — no HUD/debug-overlay/menu capability existed before this (see [ADR-0014](../decisions/0014-text-rendering-and-minimal-ui.md)). Scope: screen-space `Text` only (no world-space billboards, panels, buttons, or rich text yet), but **arbitrary developer-supplied fonts work from the start**, not as a deferred follow-up — imported via the existing `engine import` command, lazily rasterized and cached per content hash exactly like imported meshes/textures already are, alongside one embedded fallback font so text renders with zero asset authoring. `games/sandbox`'s HUD ("Pickups: N/3") is the concrete proof, using a real imported custom font, not just the fallback. Per [ADR-0001](../decisions/0001-rust-3d-from-scratch.md) and the engine's own thesis, the UI layer sits strictly on top of the same CLI/API surface everything else uses (both `engine render` and `engine play` share the one `RenderContext::draw()` path) — never a capability's only entry point, satisfied automatically rather than needing a separate design pass.

## Audio playback baseline — **done** (2026-09-01)

`engine_assets::import_audio` content-addresses wav/ogg files verbatim (no parsing at import time — mirrors the font importer), wired into `engine import`'s extension dispatch. A new `engine-audio` crate provides an `AudioSource` component (scene-authored looping music) and a Lua `engine.play_sound(clip, volume)` binding (one-shot SFX), both driven by `audio_step` — registered into `SystemRegistry` as `"audio"` like `"physics"`/`"animation"`, not auto-run. `engine mix <scene> --to <file.wav>` is audio's equivalent of `engine render`: deterministic, no real device, golden-WAV-testable, a 7th MCP tool (`weft_mix`). `engine play` opens a real device gracefully — no device available just means no sound, never a crash (confirmed directly in this sandbox, which has none). Scene files gained a fourth top-level `[audio]` table (`master`/`music`/`sfx` volumes, default `1.0`). `games/sandbox`'s `pickup.lua` fires a real vendored CC0 sound effect on collect. Spatial audio, pause/resume-after-start, and real-time fades are explicitly out of scope — see [ADR-0016](../decisions/0016-audio-playback-baseline.md).

## Animation data pipeline — **done** (2026-09-01)

The vertex format gained no new fields at all — skin/skeleton/animation data live in new, separate content-addressed asset types (`SkinData`/`Skeleton`/`AnimationClip`), joined with a mesh's plain geometry only at GPU-upload time, so no existing stored mesh's content hash changed (see [ADR-0015](../decisions/0015-animation-data-pipeline.md)). `gltf_import.rs` now reads skin/animation data (at most one of each per file); a new `engine-anim` crate provides `Animator` and a pure, deterministic clip-sampling function; `engine-render` gained a third shader/pipeline pass doing GPU skinning via a storage-buffer joint-matrix palette. Proven end-to-end through the real CLI/render surface: a hand-built skinned fixture's forearm joint matches a hand-derived 45°-rotation computation exactly at a known tick, and `engine render` visibly bends the mesh, pinned as a golden-image fixture. Blending, state machines, multiple simultaneous clips, root motion, and IK remain out of scope — Tier 3+, per ADR-0015's "revisit when".

## Expanded scripting API — **done** (2026-09-01)

`engine-script`'s Lua dispatch mechanism ([ADR-0006](../decisions/0006-scripting-and-hot-reload.md)) is built and tested. A script can draw deterministic randomness (`engine.random`/`engine.random_int`), despawn itself or another entity (`engine.despawn`), see any other entity via `engine.query` (see [ADR-0012](../decisions/0012-expanded-scripting-api.md)), and read live keyboard state via `engine.key_held` (see [ADR-0013](../decisions/0013-live-script-input-and-generalized-keycode.md)) — the fourth gap ADR-0012 originally deferred, closed once `engine-cli`'s live `play` loop actually dispatched scripts to give it a live consumer. `games/sandbox` now uses scripts for real gameplay, not just batch fixtures: three pickups in `playground.toml` run `scripts/pickup.lua`, combining `engine.key_held`/`engine.query`/`engine.despawn` to let the player collect them (walk close, hold E) — see ADR-0013's follow-up note.

## Generalize keyboard input — **done** (2026-09-01)

`engine_core::KeyCode` grew from a fixed six-variant enum (W, A, S, D, Space, Escape) to a practical full keyboard set — A–Z, digits, arrows, Enter/Tab/Space/Escape, and left/right Shift/Control/Alt — see [ADR-0013](../decisions/0013-live-script-input-and-generalized-keycode.md). Mouse and gamepad support are a different kind of work (new device classes, not just more keys) and stay in Tier 4 until something concrete needs them.

---

**Tier 1 is now fully closed** — every item above is done. As ever, this doesn't mean Tier 2 is now mandatory next: pull whichever item, from whichever tier, a concrete need actually points to (see the framing note at the top of this file and in root `ROADMAP.md`).

Next: [Tier 2 — Visual & gameplay realism](tier-2-visual-and-gameplay-realism.md)
