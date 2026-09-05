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

**A 2026-09-04 follow-up audit** (asked: is this arc's scope compatible with eventually shipping full, high-quality-graphics games, not just proving the pipeline works?) confirmed the items ADR-0019 already named as real, and reprioritized them — environment lighting now ranks above spot lights/more-lights, since it gates whether *any* content looks right rather than how many lights a scene can have. It also surfaced that the lighting bind-group layout is closer to a hard ceiling than "revisit when a scene needs more" implies. Two further gaps it found (no mipmaps, no HDR/tonemapping) predate this arc entirely and live in [Tier 3](tier-3-polish-and-feel.md) instead, reprioritized there.

- **Environment/IBL lighting** — the flat `ambient = 0.15` term remains a placeholder. **Reprioritized above the items below**: with zero ambient response outside direct light, metallic surfaces in shadow read as near-black, which breaks the "realistic" goal sooner than light count or spot lights do. Revisit before authoring much metal-heavy content.
- **The lighting bind group is already at wgpu's 4-bind-group ceiling**, not just "no consumer needs more yet." ADR-0019 folded lights, the shadow map, and the shadow sampler into one group specifically because a 4th group wouldn't fit (`shader.wgsl:48-50`). The next lighting feature — IBL textures, cascaded shadow maps, a point-light shadow cubemap — can't just add a bind group; it has to grow entries within the existing ones or restructure. Worth deciding on a storage-buffer-based light list (removes the fixed 4-light cap and gives headroom for new entries) *before* more scenes get authored assuming ≤4 lights, rather than after.
- **Spot lights** — no concrete consumer needs one yet.
- **More than 4 lights per scene, or more than one shadow-casting light** — `RenderError::TooManyLights`/`MultipleShadowCasters` are structured errors, not silent truncation.
- **Point-light shadows** — would need a 6-pass cubemap; only directional shadow casting exists.
- **Scene-bounds-fitted or cascaded shadow volumes** — today's shadow volume is a fixed-extent orthographic frustum (`SHADOW_ORTHO_HALF_EXTENT` in `gpu.rs`) centered on the camera target. Watch for this breaking (shadows vanishing past the frustum edge) the first time a level's play area outgrows the sandbox playground, not just when cascades get requested as a feature.
- **Per-object shadow-casting opt-out** — every drawable casts a shadow unconditionally.
- **Soft/PCF-filtered shadows** — today's shadow sampling is single-tap comparison sampling; shadow edges will read as aliased even once everything else here is fixed.
- **Per-frame buffer/bind-group pooling (ADR-0019's Phase 6) is entity-keyed but not yet exploited by the one-shot render paths** (`render_scene`/`render_scene_with_context`), which never get a "next frame" to amortize across.

## Physics gameplay substrate — **done** (2026-09-03)

See [ADR-0018](../decisions/0018-physics-gameplay-substrate.md) for the full design record (the phased plan that built this, `physics-substrate-plan.md`, is retired — `git log`/the ADR are its permanent record, same pattern `debt-cleanup-plan.md` used).

`PhysicsState` now exposes raycasts (`cast_ray`), sensor overlap queries (`overlapping`), and a kinematic character-controller mechanism (`move_character`); `Collider` gained `sensor`/`membership`/`filter`; `ColliderShape` gained `Capsule`; `BodyType` gained `KinematicPositionBased`. `games/sandbox`'s player is a real capsule character controller (WASD + jump, wall sliding, ground snapping) instead of a force-driven rolling sphere, and its pickups are real sensor triggers instead of hand-rolled distance math.

**Still open, not silently lost:**

- **Velocity-based kinematic bodies** — only position-based kinematics exist; no concrete consumer has needed "set a velocity, let rapier integrate it" yet.
- **A generic Lua-exposed shape-cast** — `engine.raycast` exists; a swept-shape query (as opposed to a point ray) does not.
- **Scene-authorable character-controller tuning** — `KinematicCharacterController::default()`'s fixed tuning (no autostep, 45° slope limits) isn't exposed as scene fields yet; revisit when a ramp/staircase/moving-platform case needs it.
- **A named collision-layer registry** — `Collider.membership`/`filter` are raw `u32` bitmasks, not named layers.

## Multi-mesh / multi-part asset import — **done** (2026-09-04)

`engine import` now accepts any number of meshes/primitives per glTF file instead of hard-erroring past one of either — see [ADR-0020](../decisions/0020-multi-mesh-gltf-import.md) for the full design record (Phase 18 in `completed-phases.md`).

**Still open, not silently lost:**

- **No parent/child transform component exists**, so a multi-part asset imports as flat sibling entities sharing one baked-in relative layout, not a hierarchy — moving/animating one part independently of its siblings isn't supported. Revisit when a real scene needs that.
- **At most one mesh node per file may carry a skin** (unchanged from ADR-0005/ADR-0015). A multi-part *skinned* character (several skinned siblings animated in lockstep) isn't supported — no concrete consumer needs one yet, and `Animator`'s per-entity playback state has no synchronization mechanism across siblings.
- **A mesh referenced by more than one node (instancing)** is a structured error, not supported — no concrete consumer has needed it yet.

---

Previous: [Tier 1 — Foundational](tier-1-foundational.md) · Next: [Tier 3 — Polish & feel](tier-3-polish-and-feel.md)
