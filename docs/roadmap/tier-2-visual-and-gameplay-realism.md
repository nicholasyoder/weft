# Tier 2 — Visual & gameplay realism

> **This is a suggested grouping, not a queue.** Pull whichever item a concrete need actually points to next, from any tier — don't treat tier order as a commitment. See [`../../ROADMAP.md`](../../ROADMAP.md) for the full framing.

The "realistic graphics" half of the original ask, plus the physics/gameplay-query surface real levels and encounters lean on. These build naturally on [Tier 1](tier-1-foundational.md)'s foundations — a script that raycasts needs the expanded scripting API; a lit PBR object needs the material/light model below to exist first.

---

## PBR material model

`Material` today is a flat base color plus one optional texture, and the shader is Lambertian diffuse plus a flat ambient term — not physically based at all. Add roughness/metallic (and, once there's a concrete need, emissive) to both the component data and the shader. Matching import support in `engine-assets` (currently base-color-only, per [ADR-0005](../decisions/0005-asset-pipeline.md)) is part of the same piece of work, not a separate one later — a PBR shader with nothing feeding it real material data isn't useful on its own.

## Normal mapping

Structurally blocked today: `Vertex` carries `position`/`normal`/`uv` with no tangent. Pairs naturally with the PBR work above, since both touch the vertex format and the material/shader pipeline in the same pass.

## Multiple, scene-authorable lights

The one directional light that exists is a hardcoded constant in `gpu.rs`, not a component — no scene can place, color, or add a second one, and there's no point/spot light at all. Needed before any scene can look like more than "one fixed sun."

## Shadow mapping

No shadow pass exists yet. Natural to sequence after multi-light support lands, since a shadow implementation needs to decide which light(s) actually cast shadows.

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
