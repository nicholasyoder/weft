# ADR-0020: Multi-mesh / multi-primitive glTF import

- **Status**: accepted
- **Date**: 2026-09-04

## Context

ADR-0005 deliberately scoped `engine import`'s glTF support to one mesh, one primitive per file — the only real assets at the time were single-primitive generated crates. The 2026-09-04 Tier 2 reprioritization audit (see `docs/roadmap/tier-2-visual-and-gameplay-realism.md`) ranked lifting this above the remaining lighting work: a scene already looks reasonable under today's single-sun lighting, but it can't contain a normal composite prop or character at all, since almost nothing exported from Blender (or a generative pipeline) is a single mesh/primitive. This ADR covers lifting that restriction.

## Decision

**`engine import` now accepts any number of meshes/primitives reachable from a glTF file's default scene**, importing each primitive as its own `ImportedPart` (mesh + metallic-roughness PBR material). `engine_assets::import_gltf` walks the scene graph once (`collect_mesh_nodes`), collecting every node that references a mesh along with its accumulated world transform and (optional) glTF skin — replacing the old single-target tree walks that only ever looked for one mesh.

**Each part becomes its own flat sibling entity, not a hierarchy.** This engine has no parent/child transform component yet (confirmed: none exists in `engine-core`/`engine-scene`/`engine-render`), so there's no entity to hang an internal hierarchy off of. Instead, each unskinned part's mesh-local-to-model-root transform (the node's world transform in the source file) is baked into its own vertex data at import time — generalizing ADR-0005 decision 4 from "the one mesh" to "each part" — and `engine import`'s emitted scene fragment gives every sibling entity an identical `Transform`. A scene author moves/places the whole imported object by editing every sibling's `Transform` identically. Moving or animating one part independently of its siblings is not supported.

**At most one mesh node per file may carry a glTF skin.** `Animator` (`engine-anim`) is a per-entity component that owns its own playback state (`time`/`playing`/`speed`) and writes that entity's `JointPalette` independently every tick. Two sibling entities each with their own `Animator` pointing at the same skeleton/clip could desync (different speed, drift) with no mechanism to keep them in lockstep. No concrete consumer needs a multi-part *skinned* character yet, so the file-level "one skin" restriction from ADR-0005/ADR-0015 stays exactly as-is — it just now coexists with any number of *unskinned* sibling parts in the same file (e.g. a skinned body plus a separate static prop mesh). A skinned mesh node's primitives (if it has more than one) each still get their own per-primitive skin data (`JOINTS_0`/`WEIGHTS_0` are vertex attributes, so they can't be shared), but share one skeleton/clip and — per the paragraph above — would each need their own `Animator` if the scene wants them all animated, with the desync risk that implies. No fixture exercises this specific combination; it's a known, documented gap rather than a silent one.

**A mesh referenced by more than one node (instancing) is a structured `ASSET_GLTF_UNSUPPORTED` error**, not a silent "pick the first reference" — same scope-limiting posture as every other rejection in this pipeline (ADR-0005's own primitive/attribute checks, ADR-0015's "more than one skin").

## Alternatives considered

- **A parent/child transform component, so a multi-part asset is one entity with children**: the more "correct" long-term shape, but there is no concrete consumer needing independently-movable parts yet, and adding a hierarchy component is a bigger, more invasive change than this import-side fix alone justifies. Revisit together if/when a real need surfaces (see Revisit when).
- **A synced "shared animator" mechanism for multi-part skinned characters**: no concrete consumer has a multi-part skinned character yet; today's one-skinned-mesh-node restriction already covers every skinned asset built so far (`skinned.gltf`, the sandbox player). Building synchronization machinery speculatively would be exactly the kind of premature abstraction this roadmap's practice notes warn against.
- **Silently allowing mesh instancing by importing the first reference only**: rejected for the same reason ADR-0005 rejected synthesizing missing normals/indices — it hides a real gap (two "different" placements the importer silently collapsed to one) instead of surfacing it.

## Consequences

- `engine_assets::ImportedAsset` becomes `{ parts: Vec<ImportedPart>, skeleton_hash, clip_hash }` instead of one flat struct; `ImportedPart.skin_hash` is per-part (not file-level), since skin data is a vertex attribute. This is a breaking API change to `engine-assets`/`engine-cli`'s `ImportResult` (`mesh_hash`/`skin_hash` → `mesh_hashes`/`skin_hashes`) and `engine-mcp`'s `weft_import` JSON response — no compatibility shim, consistent with this codebase's practice of not carrying dead back-compat paths.
- `engine import`'s emitted fragment is byte-identical to before for the (still overwhelmingly common) single-part case; a multi-part file emits one `[[entity]]` block per part plus a short comment explaining the "same `Transform` on every sibling" convention.
- Full PBR/multi-part assets can now actually contain a normal imported character or composite prop — the concrete gap this ADR closes, per the Tier 2 audit.

## Revisit when

- A real scene needs to move, animate, or otherwise address one part of an imported multi-part asset independently of its siblings — that's the trigger for actually building a parent/child transform component, not before.
- A real asset needs a multi-part *skinned* character (e.g. a body + separately-materialed accessory, both animated in lockstep) — that's the trigger for either a shared-animator mechanism or an explicit "these entities' `Animator`s must stay in sync" authoring convention.
- A real workflow actually wants mesh instancing (the same mesh placed at several nodes) — today's rejection would need to become support for it instead.
