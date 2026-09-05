# ADR-0019: PBR materials, multi-light, and shadow mapping

- **Status**: accepted
- **Date**: 2026-09-04

## Context

Tier 2's ["Visual & gameplay realism"](../roadmap/tier-2-visual-and-gameplay-realism.md) named four open items: `Material` was a flat base color plus one optional texture ("Full PBR is not a goal," per its own doc comment), `shader.wgsl`/`skinned_shader.wgsl` both hardcoded identical Lambertian-plus-flat-ambient math against a single `const LIGHT_DIR` baked into `gpu.rs` — not a component, not scene-authorable, no second light or point/spot light possible — `Vertex`/`SkinnedVertex` carried no tangent (normal mapping structurally blocked), and no shadow pass existed at all.

[`docs/roadmap/visual-realism-plan.md`](https://github.com/nicholasyoder/weft/blob/07628d3/docs/roadmap/visual-realism-plan.md) (now retired — this ADR and `completed-phases.md`'s entry are its permanent record, the same pattern `physics-substrate-plan.md`/`debt-cleanup-plan.md` established) split the work into 7 phases: PBR material model, metallic-roughness texture import, normal mapping, a scene-authorable `Light` component with a multi-light shader loop, a shadow-map pass, per-frame uniform-buffer/bind-group pooling, and this docs closeout.

## Decisions

**PBR shading: metallic-roughness workflow, Cook-Torrance GGX / Schlick-Fresnel / Smith-visibility, one flat ambient term kept.** The standard real-time formulation (Karis 2013). `Material` gained `roughness: f32` (default `1.0`) and `metallic: f32` (default `0.0`) rather than a combined "PBR" flag — defaults chosen so an unmodified existing scene renders as close to its old Lambertian look as the BRDF change allows. The flat `ambient = 0.15` term stays as a placeholder for missing IBL; no environment lighting is a goal of this arc.

**Texture bind group grows entries, not bind groups.** `texture_bind_group_layout` went from 2 entries to 4 across Phases 2-3 (base color, metallic-roughness, normal, one shared sampler) rather than a bind group per texture type — confirmed against wgpu 30's real `max_bind_groups: 4` default, which the skinned pipeline already sat at.

**Lights: a fixed-size `array<GpuLight, 4>` in a uniform buffer, not a storage buffer.** No concrete scene needs more than a handful of lights; a `GpuLight` built from `vec4<f32>`s is inherently 16-byte-strided, satisfying WGSL's uniform-array alignment with no manual padding — unlike joints (genuinely unbounded per-skin), a fixed cap fits the guaranteed 64 KiB uniform binding size.

**One shared "lighting" bind group carries lights, the shadow map, and the shadow comparison sampler together**, keeping both pipelines' bind-group count at exactly 4 (uniform, texture, [joint], lighting) — at wgpu's confirmed limit, never over it.

**Tangents: a new sibling asset type (`engine_assets::tangent::TangentData`), never a field on `MeshData`.** `MeshData`'s bincode encoding has no versioning scheme; adding a field there would corrupt every already-committed mesh blob. Mirrors ADR-0015's `SkinData`/`Skeleton` precedent exactly. `Vertex`/`SkinnedVertex` gained an unconditional `tangent: [f32; 4]` (xyz + handedness sign), always present even for meshes with no stored `TangentData` (a documented dummy value) — avoids a combinatorial explosion of tangent/no-tangent pipeline variants.

**Tangent generation: read glTF's `TANGENT` accessor when present, generate on import otherwise, never at render time.** `tangent::generate` runs only when the imported material has a normal map assigned.

**Lights: `Light` with an explicit `direction: Vec3` for directional lights**, not derived from `Transform.rotation` — mirrors `Camera.target`'s existing "explicit is easier to hand-author than a quaternion" reasoning. Point lights use `Transform.position`. Spot lights are out of scope — no concrete consumer needs one.

**Zero-`Light`-entities scenes stay lit, not pitch black.** `extract_scene` synthesizes one fallback directional light (the old hardcoded direction/color/intensity) when no `Light` entity exists, so every pre-existing scene file keeps rendering unchanged.

**Light-count and shadow-caster-count ambiguity are structured errors, not silent truncation.** More than 4 `Light` entities is `RenderError::TooManyLights` (`RENDER_TOO_MANY_LIGHTS`); more than one shadow-casting `Light` is `RenderError::MultipleShadowCasters` (`RENDER_MULTIPLE_SHADOW_CASTERS`); a non-directional shadow caster is `RenderError::UnsupportedShadowCaster` (`RENDER_UNSUPPORTED_SHADOW_CASTER`) — matches `extract_scene`'s existing `NoCamera`/`MultipleCameras` precedent.

**Shadow mapping: exactly one directional shadow-casting light, a fixed-extent orthographic volume centered on the main camera's `target`, a single 2048×2048 `Depth32Float` map, no cascades.** Point-light shadows and frustum/scene-bounds-fitted or cascaded volumes are real quality gaps left open on purpose — a first working shadow pass was the actual goal, not a general-purpose shadow system.

**Shadow bias lives at the receiver (fragment shader), not the rasterizer.** A rasterization-level `DepthBiasState` on the shadow pipeline was tried first and rejected — see the gotcha below.

**Per-frame uniform-buffer/bind-group pooling closes `known-issues.md`'s deferred debt item, as its own phase (6), not deferred again.** `known-issues.md` explicitly named this arc's fourth render pass (shadows) as the trigger to revisit it, since shadows *double* the per-drawable fresh-buffer-and-bind-group cost. `RenderContext` gained three entity-keyed pools (`HashMap<hecs::Entity, (Buffer, BindGroup)>`): the main pass's per-drawable `Uniforms`, the shadow pass's own (different `view_proj`, so a separate pool from the main pass), and one joint-matrix pool shared identically by both passes (same `JointPalette` content regardless of which pass draws it, so one entry serves both rather than each pass keeping its own copy). Existing entries update via `queue.write_buffer`; entities no longer in the frame's drawable set are evicted. The lights bind group's single per-frame buffer/bind-group is now built once in `RenderContext::from_core` and only its contents change per frame — every resource it references (`shadow_map_view`, the shadow sampler, now the lights buffer too) is stable across frames, so the bind group itself never needs to change shape. This only meaningfully pays off for the live/windowed path; one-shot/batch callers never get a "next frame" to amortize across, harmless-but-inert there. Deliberately no perf benchmark added (none exists to validate against) — verification is the existing golden-image tests staying byte-identical, since this is a pure allocation-pattern refactor with no intended visual change.

## Two real gotchas, found and documented, not just worked around

**Phase 3 — a fixed per-vertex tangent fallback can degenerate the shader's math.** A hardcoded fallback tangent like `(1,0,0)` can land exactly parallel to a real geometric normal (an axis-aligned box face), collapsing the fragment shader's Gram-Schmidt orthogonalization to a zero-length/NaN tangent and corrupting shading even with no normal map bound at all. Caught by actually running the golden-image fixtures, not by design review. Fixed by deriving the fallback from each vertex's own normal (`tangent::arbitrary_orthogonal`) instead of a hardcoded world-space direction.

**Phase 5 — rasterizer-level shadow bias erodes real shadows instead of just fighting acne.** A `DepthBiasState` on the shadow pipeline shifts every occluder's *stored* depth uniformly; translated through this pass's wide `SHADOW_NEAR`/`SHADOW_FAR` range into world-space units, even wgpu's small default-scale bias values washed out most of a test shadow's silhouette wherever a grazing light ray only clipped a shallow depth of the occluder. Diagnosed by dumping the raw shadow map texture and comparing occluder/receiver depths by hand. Fixed by moving all bias to the receiver side instead: `fs_main`'s own `SHADOW_BIAS` constant (`shader.wgsl`/`skinned_shader.wgsl`, currently `0.0015`) is a fixed, small comparison-depth offset applied once per receiving fragment, not compounded per rasterized occluder fragment — the shadow pipelines themselves use `wgpu::DepthBiasState::default()` (no rasterizer bias at all). Documented directly on `gpu.rs`'s `shadow_bias` local and both shaders' `SHADOW_BIAS` constant, so a future tuning pass doesn't reintroduce the rasterizer-side version and rediscover the erosion from scratch.

## Alternatives rejected

- **Unbounded storage-buffer lights** (mirroring `joint_matrices`'s pattern): no consumer needs more than 4; would just be copying the joints pattern reflexively rather than for a real reason.
- **A bind group per texture type / per shadow resource**: would exceed wgpu's confirmed 4-group ceiling once the skinned pipeline carries texture + joint + lighting groups alongside the uniform group.
- **Tangents as a field on `MeshData`**: bincode has no versioning; would corrupt every already-committed mesh blob in `games/sandbox/assets/` and test fixtures.
- **Requiring at least one authored `Light` per scene**: would silently turn every pre-existing scene fully black with no structural error to catch it — strictly worse than the synthesized-fallback approach taken.
- **Cascaded or scene-bounds-fitted shadow volumes, point-light shadows, per-object shadow-casting opt-out**: real quality gaps, none needed by a concrete scene yet — a first working single-caster directional shadow pass was this arc's actual scope.
- **A perf benchmark for Phase 6's pooling change**: none exists in this codebase to validate pooling against (same caveat `debt-cleanup-plan.md`'s own Phase 4 already noted); golden-image byte-identity is the verification instead.

## Consequences

- Every existing scene file renders without any required authoring change (unmodified `Material`/zero-`Light` scenes fall back to close-to-old defaults), while a scene that does opt in gets real PBR shading, multiple colored/positioned lights, normal-mapped surfaces, and one real shadow-casting directional light.
- `games/sandbox`'s `playground.toml` now authors a real directional "sun" `Light` with `casts_shadow = true`, proving the whole arc end-to-end in live gameplay, not just fixtures.
- The live/windowed render loop no longer allocates a fresh uniform buffer + bind group per drawable per frame per pass — the debt `known-issues.md` flagged before this arc even began is closed, not just reduced.
- Still open, not silently lost, all real quality gaps rather than oversights:
  - Spot lights.
  - More than 4 lights per scene, or more than one shadow-casting light.
  - Point-light shadows (would need a 6-pass cubemap).
  - Scene-bounds-fitted or cascaded shadow volumes (today's shadow volume is a fixed-extent ortho frustum centered on the camera target).
  - Per-object shadow-casting opt-out (every drawable currently casts a shadow unconditionally).
  - Soft/PCF-filtered shadows beyond today's single-tap comparison sampling.
  - Environment/IBL lighting (the flat `ambient = 0.15` term remains a placeholder).
  - Phase 6's pooling is entity-keyed and not yet exploited by the one-shot render paths (`render_scene`/`render_scene_with_context`), which never get a "next frame" to amortize across.

## Revisit when

- A concrete scene needs more than 4 lights, more than one shadow caster, or a point/spot light that casts shadows — that's the trigger to reconsider the fixed-cap uniform-array/single-caster design here.
- A real level's geometry outgrows the shadow pass's fixed orthographic half-extent (`SHADOW_ORTHO_HALF_EXTENT`) — that's the trigger for scene-bounds-fitted or cascaded shadow volumes.
- Environment reflections or ambient occlusion become a concrete need — that's the trigger to revisit the flat ambient term with real IBL.
- A one-shot render path (batch `engine render` over many scenes/frames) develops a real perf need — that's the trigger to extend Phase 6's pooling benefit there, or to add the perf benchmark this phase deliberately didn't.
