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

## Physics gameplay substrate

`engine-physics` proves basic dynamics work — the sandbox's rolling ball is real — but exposes almost none of the query/gameplay surface real levels and encounters need:

- **Raycast / shape-cast queries** — `PhysicsState` exposes `apply_force` only today; no way to ask "what's in front of me" or "what's under this point."
- **Triggers / sensors** — no overlap-without-collision mode exists.
- **Collision layers / filtering** — `Collider` has no group/membership/filter fields.
- **Character controller** — not built; the sandbox drives its ball with raw forces, not anything resembling a controller.
- **Kinematic bodies** — deferred explicitly in `RigidBody`'s own doc comment (nothing drives a body's pose externally yet). Needed for moving platforms and pairs naturally with a character controller.
- **Capsule colliders** — box and sphere only today; capsule is the natural shape for a character controller.

## Multi-mesh / multi-part asset import

`engine import` hard-errors (`ASSET_GLTF_UNSUPPORTED`) on anything beyond one mesh/one primitive per glTF file — a deliberate scope limit called out in [ADR-0005](../decisions/0005-asset-pipeline.md), not an oversight. Real props and characters generally aren't single-primitive files. This needs to lift before content complexity can grow much past the current crate/pillar-obstacle level.

---

Previous: [Tier 1 — Foundational](tier-1-foundational.md) · Next: [Tier 3 — Polish & feel](tier-3-polish-and-feel.md)
