# Tier 3 — Polish & feel

> **This is a suggested grouping, not a queue.** Pull whichever item a concrete need actually points to next, from any tier — don't treat tier order as a commitment. See [`../../ROADMAP.md`](../../ROADMAP.md) for the full framing.

What separates a playground from something that feels like a finished game. Real value, but nothing here blocks [Tier 1](tier-1-foundational.md) or [Tier 2](tier-2-visual-and-gameplay-realism.md), and none of it is needed for a first genuinely playable, good-looking milestone.

---

## Rendering polish

- **Anti-aliasing** — `sample_count: 1` is hardcoded in three places in `gpu.rs` today.
- **Mipmapping** — new finding, 2026-09-04 audit: every texture upload in `gpu.rs` hardcodes `mip_level_count: 1`; nothing generates a mip chain at import or runtime. Distant/grazing-angle textures will alias/shimmer regardless of any other quality work, and it's cheaper to fix before more texture content exists than after — treat this as ranking above the rest of this list, not as ordinary polish.
- **Transparency / alpha blending** — the pipeline's blend state is hardcoded off; the shader always outputs alpha `1.0`.
- **Skybox / environment lighting (IBL)** — no environment map or ambient probe exists. **Reprioritized, 2026-09-04 audit**: the ambient-response half of this (see [Tier 2](tier-2-visual-and-gameplay-realism.md)'s shadow-mapping section) ranks above spot lights/more-lights over there — pull it from here if a lighting need surfaces first, rather than waiting for the rest of this Tier-3 list.
- **Post-processing** — no tone mapping, bloom, or color grading; frames go straight from render to swapchain/output texture. **Split by the 2026-09-04 audit**: the tone-mapping/HDR-target piece specifically is closer to a correctness gap than polish once more than one light is in play — `fs_main` writes lit radiance directly into the `Rgba8UnormSrgb` swapchain format with no rolloff curve, and multi-light PBR can already produce values above 1.0 (stacked specular highlights, a bright point light up close) that hard-clip to flat white today instead of rolling off filmically. Worth pulling that piece forward independently of bloom/color-grading, which can stay lower-priority polish.
- **Particle systems** — no emitter mechanism anywhere in the engine.
- **Level of detail (LOD)** — no mesh-swap-by-distance mechanism; only matters once a scene has enough geometry for it to.

## Rendering & world scalability

New section, 2026-09-04 audit — neither item below was previously named anywhere on this roadmap.

- **No frustum culling or draw batching/instancing** — `gpu.rs`'s `draw()` issues one draw call per drawable, unconditionally, for every entity `extract_scene` returns. Fine at `games/sandbox`'s current scale; won't scale to a real level's entity count. Cheap to design for now (a culling step inside `extract_scene`) and progressively more annoying to retrofit once more systems assume "every drawable always gets uniforms built."
- **`Transform.position` is `f32` (`glam::Vec3`)** — no camera-relative-rendering or origin-rebasing plan exists. Fine for a sandbox-sized level; will show visible jitter once a level's play area gets far enough from the origin for single-precision float error to matter (roughly kilometer scale). Revisit when a level's extent starts approaching that, not before — noted now so it isn't discovered mid-level-design.

## Animation blending / state machines

Builds directly on Tier 1's animation data pipeline — sequence this after basic skeletal playback actually works, not before. Named explicitly as a future need in the engine's own history.

## Advanced physics shapes and constraints

- **Mesh / convex-hull colliders** — box, sphere, and (once Tier 2 lands) capsule cover most gameplay needs; mesh/convex colliders exist for the cases those genuinely can't approximate.
- **Joints / constraints** — hinges, fixed joints, springs. More specialized than Tier 2's gameplay substrate; add when a specific mechanic (a door, a rope, a vehicle) actually needs one, not ahead of that need.

## Save-game system

No serialization of arbitrary world state to a save slot exists today. This is a different mechanism from deterministic replay (`Recording.inputs`) — [ADR-0010](../decisions/0010-live-input-and-windowed-run-loop.md) itself calls that recording path "the Phase-0-era placeholder it's always been," not really wired to `engine play`. Finishing that wiring (or explicitly deciding it's not worth finishing) is a small adjacent cleanup worth doing alongside a real save system, not a prerequisite for it.

## Texture / mesh asset hot-reload

Scenes and Lua scripts already hot-reload under `engine run --watch`; content-addressed texture/mesh assets don't — changing a source asset produces a new content hash, which needs a scene-file edit to pick up. A genuine iteration-speed nicety, not a blocker for anything else on this roadmap.

---

Previous: [Tier 2 — Visual & gameplay realism](tier-2-visual-and-gameplay-realism.md) · Next: [Tier 4 — Ship readiness](tier-4-ship-readiness.md)
