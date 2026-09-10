# ADR-0021: Hemisphere ambient lighting

- **Status**: accepted
- **Date**: 2026-09-10

## Context

ADR-0019 shipped PBR/multi-light/shadows with a deliberate placeholder: `ambient = 0.15 * base_color.rgb`, flat and direction-independent, explicitly flagged as "revisit when environment reflections or ambient occlusion become a concrete need." A later audit of [Tier 2](../roadmap/tier-2-visual-and-gameplay-realism.md) reprioritized this above spot lights/more-lights: with zero ambient response outside direct light, metallic surfaces sitting in shadow read as near-black, which undercuts the "realistic" goal sooner than light count does.

A full cubemap/prefiltered-IBL system was considered and ruled out for this pass. The skinned pipeline is already at wgpu's confirmed `max_bind_groups` limit of 4 (uniforms / textures / lights+shadow / joint palette) — a new environment texture has no bind group to occupy without restructuring the pipeline layout. `engine-assets` also has no HDR/equirect import path at all: `import_texture` round-trips every input through 8-bit PNG, which would clip the dynamic range a real environment map needs.

## Decision

**Hemisphere/gradient ambient, folded into the existing `Lights` uniform buffer.** `Lights`/its WGSL mirror grow two `vec4` fields — `ambient_sky` (rgb + overall intensity in `.w`) and `ambient_ground` (rgb) — appended after the existing `_pad` field, which already lands the struct on a 16-byte boundary, so no new padding is needed. Zero new bind groups, zero new textures, zero new asset types.

**Fresnel/metallic-weighted, computed in `fs_main`.** The shading normal's up-component (`dot(n, vec3(0,1,0))`) blends `ambient_ground`/`ambient_sky` into a single `hemi_color`. `f0` is recomputed locally (it's only in scope inside `shade_light`, not `fs_main`) and fed through the existing `fresnel_schlick` to split `hemi_color` into a diffuse term (zeroed for full metals) and a specular/reflection-like term (base-color-tinted for metals, weighted by Fresnel). This is the actual fix for the near-black-in-shadow case: a metallic surface with zero direct light now reflects `hemi_color` tinted by its own base color, instead of a flat, angle-independent constant.

**Scene-authorable via a new top-level `[environment]` table**, mirroring `[audio]`/`AudioSettings` exactly: `EnvironmentSettings` lives in `engine-types` (producer `engine-scene`, consumer `engine-render`, same relationship `AudioSettings` has with `engine-audio`), parsed by `engine-scene`'s `EnvironmentMeta` (all fields `#[serde(default = ...)]`), inserted into `Sim.resources` by `engine_scene::load`. Hardcoded `Scenario`s (which never go through `engine_scene::load`) get the same `EnvironmentSettings::default()` fallback `SimSource::build` already gives `AudioSettings`.

**New plumbing, not reuse of an existing path.** Unlike `AudioSettings` (consumed only by `engine-audio`'s scheduler-driven `audio_step`), no rendering function previously received `Sim.resources` at all — every render entry point took `&hecs::World` only. `extract_scene`, `render_scene`, `render_scene_with_context`, `draw_to_surface`, `render_scene_to_png`, and `WindowRenderer::render` all gained an `environment: EnvironmentSettings` parameter, threaded from `sim.resources.get::<EnvironmentSettings>().copied().unwrap_or_default()` at the two real call sites (`engine-cli`'s batch `render_scene` and `live::play`'s per-frame render call).

## Alternatives considered

**Prefiltered cubemap IBL.** Physically correct, gives real reflections, but needs an HDR/equirect import pipeline (doesn't exist), an offline convolution/prefiltering step (doesn't exist), and at least one new texture+sampler binding that doesn't fit the currently-full `lights_bind_group_layout` or the per-material `texture_bind_group_layout` (a cubemap is a per-scene resource, not per-material, so cramming it into the latter is semantically wrong even if it technically fits). Rejected for this pass — a legitimate future upgrade once a concrete need for real reflections (not just ambient fill) shows up.

**Reproducing the old flat constant exactly as the new default, to keep every golden PNG byte-identical.** Impossible by construction: the old term is a flat scalar, the new term is `n`/`v`/`metallic`-dependent — no choice of `sky_color`/`ground_color`/`intensity` reproduces an angle-independent constant from a formula that varies with view/normal/material. Every ambient-lit golden was regenerated and visually reviewed instead of chasing an unreachable backward-compatibility bar.

## Consequences

Every existing golden-image fixture with ambient-lit geometry needed regeneration (not just the new `render_ambient_environment` fixture) — a one-time cost, reviewed visually scene-by-scene rather than blind-overwritten. Scene authors now get a cheap, no-texture way to make outdoor/indoor lighting read more plausibly without touching per-light setup; `games/sandbox`'s `playground.toml` authors a real `[environment]` table so the one real game exercises this outside golden fixtures. The lighting bind group's byte layout grew by 32 bytes — still well inside a uniform buffer's guaranteed minimum binding size, no practical ceiling hit.

Left open: this is a single-color-per-hemisphere approximation, not roughness-aware or reflection-capable — a mirror-smooth metal still only reflects a flat gradient, not real scene content. Ambient occlusion is untouched — a corner or crevice gets the same hemisphere response as an open surface.

## Revisit when

A concrete need for real environment reflections (not just ambient fill) shows up — e.g. a mirror-like material, a reflective water surface, or a scene where the flat-gradient approximation visibly reads as wrong rather than "good enough." At that point, revisit the cubemap-IBL alternative above, including the still-missing HDR import pipeline it depends on.
