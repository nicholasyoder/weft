# Tier 2 — Visual & gameplay realism

> **This is a suggested grouping, not a queue.** Pull whichever item a concrete need actually points to next, from any tier — don't treat tier order as a commitment. See [`../../ROADMAP.md`](../../ROADMAP.md) for the full framing.

The "realistic graphics" half of the original ask, plus the physics/gameplay-query surface real levels and encounters lean on. These build naturally on [Tier 1](tier-1-foundational.md)'s foundations — a script that raycasts needs the expanded scripting API; a lit PBR object needs the material/light model below to exist first.

---

## PBR material model — **done** (2026-09-04)

## Normal mapping — **done** (2026-09-04)

## Multiple, scene-authorable lights — **done** (2026-09-04)

## Shadow mapping — **done** (2026-09-04)

See [ADR-0019](../decisions/0019-pbr-lighting-and-shadows.md) for the full design record (the phased plan that built this, `visual-realism-plan.md`, is retired — `git log`/the ADR are its permanent record, same pattern `physics-substrate-plan.md`/`debt-cleanup-plan.md` used).

`Material` now carries `roughness`/`metallic` (plus optional metallic-roughness and normal-map textures), shaded via a metallic-roughness Cook-Torrance BRDF; a scene-authorable `Light` component supports up to 4 directional/point lights; one directional light per scene may set `casts_shadow` for a real shadow-map pass. `games/sandbox`'s `playground.toml` authors a real shadow-casting sun light, proving the whole arc in live gameplay.

**Still open, not silently lost:**

- **Spot lights** — no concrete consumer needs one yet.
- **More than 4 lights per scene, or more than one shadow-casting light** — `RenderError::TooManyLights`/`MultipleShadowCasters` are structured errors, not silent truncation.
- **Point-light shadows** — would need a 6-pass cubemap; only directional shadow casting exists.
- **Scene-bounds-fitted or cascaded shadow volumes** — today's shadow volume is a fixed-extent orthographic frustum centered on the camera target.
- **Per-object shadow-casting opt-out** — every drawable casts a shadow unconditionally.
- **Soft/PCF-filtered shadows** — today's shadow sampling is single-tap comparison sampling.
- **Environment/IBL lighting** — the flat `ambient = 0.15` term remains a placeholder.
- **Per-frame buffer/bind-group pooling (ADR-0019's Phase 6) is entity-keyed but not yet exploited by the one-shot render paths** (`render_scene`/`render_scene_with_context`), which never get a "next frame" to amortize across.

## Physics gameplay substrate — **done** (2026-09-03)

See [ADR-0018](../decisions/0018-physics-gameplay-substrate.md) for the full design record (the phased plan that built this, `physics-substrate-plan.md`, is retired — `git log`/the ADR are its permanent record, same pattern `debt-cleanup-plan.md` used).

`PhysicsState` now exposes raycasts (`cast_ray`), sensor overlap queries (`overlapping`), and a kinematic character-controller mechanism (`move_character`); `Collider` gained `sensor`/`membership`/`filter`; `ColliderShape` gained `Capsule`; `BodyType` gained `KinematicPositionBased`. `games/sandbox`'s player is a real capsule character controller (WASD + jump, wall sliding, ground snapping) instead of a force-driven rolling sphere, and its pickups are real sensor triggers instead of hand-rolled distance math.

**Still open, not silently lost:**

- **Velocity-based kinematic bodies** — only position-based kinematics exist; no concrete consumer has needed "set a velocity, let rapier integrate it" yet.
- **A generic Lua-exposed shape-cast** — `engine.raycast` exists; a swept-shape query (as opposed to a point ray) does not.
- **Scene-authorable character-controller tuning** — `KinematicCharacterController::default()`'s fixed tuning (no autostep, 45° slope limits) isn't exposed as scene fields yet; revisit when a ramp/staircase/moving-platform case needs it.
- **A named collision-layer registry** — `Collider.membership`/`filter` are raw `u32` bitmasks, not named layers.

## Multi-mesh / multi-part asset import

`engine import` hard-errors (`ASSET_GLTF_UNSUPPORTED`) on anything beyond one mesh/one primitive per glTF file — a deliberate scope limit called out in [ADR-0005](../decisions/0005-asset-pipeline.md), not an oversight. Real props and characters generally aren't single-primitive files. This needs to lift before content complexity can grow much past the current crate/pillar-obstacle level.

---

Previous: [Tier 1 — Foundational](tier-1-foundational.md) · Next: [Tier 3 — Polish & feel](tier-3-polish-and-feel.md)
