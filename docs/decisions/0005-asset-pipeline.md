# ADR-0005: Content-addressed asset store + glTF import scope

- **Status**: accepted
- **Date**: 2026-08-28

## Context

Phase 3 (see [ROADMAP.md](../roadmap/completed-phases.md)) replaces Phase 2's hardcoded cube/plane with real content: a binary asset store and glTF import, ready for the generative-asset APIs Phase 7 will eventually wire in (Meshy/Tripo3D also export glTF). The DoD is concrete — a glTF file imports and renders correctly via `engine render`, and re-importing the same file produces the same content hash with no spurious diff/churn — which forces two decisions: how assets are stored/addressed, and how much of glTF's surface is worth importing on day one.

## Decision

**1. A content-addressed store, checked into git, laid out like git's own object store** (`assets/<hash[0:2]>/<hash>`, no file extensions — the referencing component already knows how to interpret a hash). This is the direct mechanism behind "no spurious diff/churn": identical content always lands at the same path, so re-importing an unchanged file writes nothing new (proven directly in `crates/engine-cli/tests/import.rs`). Checking the store into git (rather than treating it as a gitignored build cache) keeps the whole scene — text and binary — reproducible from one clone, consistent with the project's "text-diffable, git-mergeable" thesis for the parts that *do* change; the binary blobs themselves rarely change once imported, and when they do it's a new hash, not a diff.

**2. Import scope is deliberately narrow: one mesh, one primitive, base-color material only, no animation.** *(The "one mesh, one primitive" half of this is superseded by [ADR-0020](0020-multi-mesh-gltf-import.md) as of 2026-09-04 — `engine import` now accepts any number of meshes/primitives. The rest of this decision — required attributes, base-color-only material at the time — was itself superseded piecemeal by later phases; see ADR-0019 for full PBR.)* A glTF file with more than one mesh or more than one primitive is rejected with a structured error (`ASSET_GLTF_UNSUPPORTED`), not silently merged or truncated — same principle as Phase 2's `NoCamera`/`MultipleCameras`. POSITION, NORMAL, and indices are required; TEXCOORD_0 defaults to `[0,0]` per vertex if absent. Only `baseColorFactor`/`baseColorTexture` are read — metallic/roughness/normal/emissive maps and animation channels/samplers are not imported at all. This matches the actual target use case (Phase 7's generative APIs produce single-mesh, single-material assets) rather than building general-purpose glTF support speculatively.

**3. Textures are always decoded and re-encoded to PNG on import**, regardless of source format (the `image` crate handles PNG/JPEG input). This means the render-side loader (`crates/engine-render/src/gpu.rs`) has exactly one decode path for every stored texture, whether it came from a loose image file or an embedded glTF image.

**4. A glTF node's accumulated world transform (from the default scene's root down to the node referencing the mesh) is baked into the stored vertex data at import time**, not carried as a separate offset applied at render time. Some exporters (both `Box` and `BoxTextured` sample models, from legacy COLLADA2GLTF export) put a Z-up-to-Y-up correction matrix on a wrapper node rather than the mesh node itself; baking it in means the stored mesh is already correct in the engine's coordinate space and the renderer never needs to know a mesh asset came with node-hierarchy baggage.

**5. The mesh binary format is `bincode`-encoded plain data** (`engine_assets::mesh::MeshData { positions, normals, uvs, indices }`), not a hand-rolled byte layout. `bincode`'s default config is deterministic for this shape (no maps), which is all the content-addressing scheme needs, and it avoids a custom encoder/decoder that would need its own correctness tests.

## Alternatives considered

- **Supporting multi-primitive/multi-material glTF meshes now**: rejected as speculative scope — no real multi-part asset exists yet to justify the added complexity (per-primitive material assignment, multiple `MeshRef`/`Material` pairs per imported file). Revisit when one actually shows up (see Revisit when).
- **A gitignored asset cache instead of a checked-in store**: would require a separate fetch/rebuild step for anyone cloning the repo, undermining the "one clone, everything works" property the rest of the engine already has (scene files, ADRs, fixtures). Checked-in blobs cost repo size but keep the property.
- **Computing flat per-triangle normals when NORMAL is absent, or synthesizing indices for non-indexed primitives**: rejected in favor of a structured error — most authored/exported glTF already satisfies these, and silently synthesizing data hides a real gap in what got imported (same reasoning as requiring exactly one camera in Phase 2).
- **Applying node transforms at render time instead of at import time**: would mean `MeshRef`/`MeshData` alone isn't enough to place a mesh correctly, forcing every future consumer of the mesh format to also know about glTF's node hierarchy. Baking it in at import time keeps `engine_assets::mesh::MeshData` self-contained.

## Consequences

- `engine-assets` stays renderer-agnostic (no wgpu/bytemuck types) and `engine-render` depends on it, not the reverse — the same layering discipline as `engine-scene` sitting below `engine-cli`.
- `MeshKind` (in `engine-render::components`) gained an `Asset(String)` variant alongside `Cube`/`Plane`; `Material` gained an optional `texture: Option<String>` field. Both are additive — existing hand-authored scene files (`render_basic.toml`) needed no changes.
- Full PBR, multi-part assets, and animation remain unimplemented. This is a real gap, not an oversight: it's written down here so a future session doesn't need to rediscover it by reading `gltf_import.rs`.

## Revisit when

- ~~A real asset (hand-authored or generated) needs more than one mesh/primitive per file...~~ **Done, see [ADR-0020](0020-multi-mesh-gltf-import.md) (2026-09-04).**
- Animation is needed by an actual test game (Phase 6+ gameplay substrate) — import glTF animation channels/samplers then, not before.
- Metallic/roughness or normal-map textures are needed for visual fidelity a test game actually requires — extend `Material` and the shader together at that point, following the same "simple-lit is fine until it isn't" reasoning as ADR/Phase 2.
