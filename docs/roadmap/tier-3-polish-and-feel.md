# Tier 3 — Polish & feel

> **This is a suggested grouping, not a queue.** Pull whichever item a concrete need actually points to next, from any tier — don't treat tier order as a commitment. See [`../../ROADMAP.md`](../../ROADMAP.md) for the full framing.

What separates a playground from something that feels like a finished game. Real value, but nothing here blocks [Tier 1](tier-1-foundational.md) or [Tier 2](tier-2-visual-and-gameplay-realism.md), and none of it is needed for a first genuinely playable, good-looking milestone.

---

## Rendering polish

- **Anti-aliasing** — `sample_count: 1` is hardcoded in three places in `gpu.rs` today.
- **Transparency / alpha blending** — the pipeline's blend state is hardcoded off; the shader always outputs alpha `1.0`.
- **Skybox / environment lighting (IBL)** — no environment map or ambient probe exists.
- **Post-processing** — no tone mapping, bloom, or color grading; frames go straight from render to swapchain/output texture.
- **Particle systems** — no emitter mechanism anywhere in the engine.
- **Level of detail (LOD)** — no mesh-swap-by-distance mechanism; only matters once a scene has enough geometry for it to.

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
