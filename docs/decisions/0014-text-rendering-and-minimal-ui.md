# ADR-0014: Text rendering and a minimal UI layer

- **Status**: accepted
- **Date**: 2026-09-01

## Context

Tier 1's capability audit flagged `engine-render`'s complete absence of a glyph/font pipeline as the cheapest-to-build, most-expensive-to-retrofit item still open: no HUD, debug overlay, menu, or dialogue box is possible without one, and building it before Tier 2 piles PBR/shadows/normal-mapping onto the render pipeline means less to integrate a new draw pass against later. `games/sandbox` already had real gameplay (WASD movement, camera-follow, three script-driven pickups collectible with E) but zero on-screen feedback — nothing told the player how many pickups remained.

The user picked this item explicitly over audio, the animation pipeline, and Tier 2's physics substrate, then added one requirement mid-planning that reshaped the design: arbitrary developer-supplied fonts had to work from the start, not as a deferred "custom font import" follow-up the way multi-mesh glTF import was deferred after Phase 3. Per the standing [[project-weft-asset-generation]] constraint — human-authored assets must work identically to built-in/generated ones — a text feature that only ever rendered one hardcoded font wouldn't be done.

Before this ADR, `engine-render` had: one `wgpu` render pass, one shader, opaque-only geometry (`blend: None` everywhere), a fresh per-object uniform buffer per draw, no orthographic/screen-space projection anywhere, and no font/text crate in the workspace.

## Decision

**Scope for v1**: screen-space HUD text only (`Text` has no `Transform` — no world-space billboards yet), arbitrary developer-supplied fonts via the existing content-addressed asset pipeline, plus one embedded fallback font so nothing requires asset authoring to render text at all. No panels, buttons, 9-slice, rich text, or word-wrapping.

**Font rasterization: `fontdue`**, not `glyphon`/`cosmic-text`/`wgpu_glyph`. Those bring their own end-to-end wgpu text-rendering pipeline, which would fight this engine's one-pipeline-hand-rolled-on-focused-libraries architecture (ADR-0001). `fontdue` only rasterizes glyphs to coverage bitmaps; the atlas, pipeline, and draw-call integration are hand-rolled in `crates/engine-render/src/text.rs`, matching every other subsystem in this crate.

**New component** (`crates/engine-render/src/components.rs`):
```rust
pub struct Text {
    pub content: String,
    pub x: f32,      // pixel position, top-left origin, screen space
    pub y: f32,
    pub size: f32,   // pixel font size
    pub color: [f32; 3],
    pub font: Option<String>,  // content hash into engine-assets; None = embedded fallback
}
```
Registered into `engine-cli::registry::components()` exactly like `Camera`/`Material` — an engine-owned capability, not sandbox-specific.

**Font import reuses the existing `engine import` command** rather than inventing a new one: `import_asset`'s extension dispatch (`crates/engine-cli/src/lib.rs`) gained a `.ttf`/`.otf` arm calling a new `engine_assets::import_font`, which content-addresses the raw bytes verbatim — no parsing at import time. `engine-assets` stores bytes; `engine-render` is the only crate that understands font formats, and only does so lazily, at draw time. This mirrors `import_texture`'s bytes-in-hash-out shape minus the decode/re-encode step (there's no normalization to do for a font the way there is for an image format). Zero new CLI/MCP surface: `weft_import` already wraps `import_asset` generically.

**Atlas — lazily built and cached per font hash**, mirroring the existing `mesh_cache`/`texture_cache` pattern in `RenderContext`, not one atlas baked in eagerly at startup for every font a scene might reference. `RenderContext` gained `font_cache: HashMap<String, GlyphAtlas>` (keyed by content hash, exactly like `texture_cache`) plus one `default_atlas: GlyphAtlas` built eagerly from the embedded fallback font in `RenderContext::from_core`. On first use of a `Text.font = Some(hash)`, the bytes are fetched from `engine_assets::AssetStore` (same lazy-fetch-and-cache shape `draw()` already used for textures), rasterized (printable ASCII 0x20–0x7E only) via `fontdue` at one fixed base pixel size (48px) into an `R8Unorm` atlas texture via simple shelf packing, cached in `font_cache`. `Text.size` scales each glyph quad geometrically at layout time against that base size — no SDF, so quality softens the further a given size drifts from the base; an SDF atlas is a natural Tier 3 polish upgrade, not built here.

**Reused, not duplicated, GPU layout**: the glyph atlas's texture+sampler bind group reuses the *same* `texture_bind_group_layout`/`sampler` the 3D pipeline already built for imported textures — an `R8Unorm` atlas fits that layout's `Float{filterable: true}` shape exactly, so no second texture layout was needed. Only a new *uniform* bind group layout (`ui_bind_group_layout`, one small `screen_size` uniform, `VERTEX`-only visibility) and a new pipeline layout combining the two were added.

**Rendering extends the existing single pass**, per the tier item's own stated constraint (a window must never be a capability's only entry point, per ADR-0001/ground rule 2) — no second render pass:
- `extract_scene` gains a `world.query::<&Text>()`, sorted by entity id (ADR-0002 determinism), into `TextDrawable`s.
- A new `ui_shader.wgsl` maps pixel-space vertex positions straight to NDC (`(pixel / screen_size) * 2 - 1`, y-flipped for top-left-origin screen space) — no model/view/projection matrix, unlike the 3D shader.
- A new UI pipeline variant is alpha-blended (`BlendState::ALPHA_BLENDING` — the first alpha blending anywhere in this codebase; the 3D pipeline is `blend: None`) with `depth_compare: Always, depth_write_enabled: false`, so HUD text always draws on top of the 3D scene regardless of the shared depth buffer's contents, without a second pass.
- `RenderContext::draw()` appends a UI batch after the existing 3D `Drawable` loop, inside the same render pass: `TextDrawable`s are grouped by resolved font key (a `BTreeMap` for stable iteration order, matching ADR-0002's determinism discipline elsewhere), laid out into one combined vertex+index buffer per font group, and drawn with one draw call per group — a HUD using two fonts costs two draw calls, not one per `Text` entity.
- Both `render_scene_with_context` (offscreen PNG) and `draw_to_surface` (live windowed) already funnel through this one `draw()`, so both pick up text rendering for free with no separate code path.

**`games/sandbox` proof**: a new `Pickup` marker component (game-owned, same extension pattern as `PlayerControl`/`CameraFollow`) tags the three pickup entities; a new `hud_system` counts remaining `Pickup`-tagged entities each tick and writes `"Pickups: N/3"` into the scene's `Text` entity. Counting via a game-owned marker component — not scanning `Script`/`pickup.lua` internals — keeps the count correct regardless of how a pickup happens to be implemented. The `hud` entity's `Text.font` is a real imported custom font (`games/sandbox/assets-src/PressStart2P-Regular.ttf`, OFL-licensed, visually distinct pixel-art style from the engine's embedded fallback), not left as `None` — concrete end-to-end proof that arbitrary developer-supplied fonts work, not just the built-in default. No new Lua/`engine.*` API was needed: `hud_system` is a plain Rust system: `pickup.lua` is untouched.

## Alternatives considered

- **`glyphon`/`cosmic-text` or `wgpu_glyph`** instead of hand-rolling on `fontdue`. Rejected: these bring their own complete wgpu text pipeline (atlas management, its own render pass/pipeline ownership), which doesn't compose cleanly with `RenderContext`'s existing single-pipeline-cache/single-pass architecture — would mean either running a second, separately-owned pipeline outside this crate's established patterns, or fighting the library's own assumptions to fold it in.
- **One embedded font only, custom fonts deferred** — the original plan, mirroring how Phase 3 deferred multi-mesh glTF import. Superseded mid-planning by the user's explicit requirement and the [[project-weft-asset-generation]] constraint: unlike multi-mesh import (a scope narrowing within an already-working feature), a text feature that could only ever render one hardcoded font wouldn't satisfy "human-authored assets work identically to built-in ones" at all.
- **A second render pass for UI**, loaded after the 3D pass with `LoadOp::Load`. Rejected in favor of appending to the existing pass: fewer pass transitions, and `depth_compare: Always` already gets "always on top" without needing a second pass's simpler (no depth test) semantics.
- **An SDF (signed distance field) glyph atlas** instead of a plain coverage bitmap, for crisp text at any scale. Rejected for v1 as unnecessary complexity: a fixed-base-size raster atlas is adequate for a first HUD/debug-overlay pass, and SDF is a natural, isolated Tier 3 polish upgrade if text quality at extreme sizes ever becomes a real problem.

## Consequences

- `engine-render`'s render pass now does alpha blending for the first time — future opaque-geometry work (Tier 2's PBR pass, for instance) needs to keep the 3D pipeline's `blend: None` in mind if it ever wants transparency for 3D materials too; that would need its own blend-state decision, not inherited from this one.
- `RenderContext` grew two more long-lived caches (`font_cache`, plus the eager `default_atlas`) alongside `mesh_cache`/`texture_cache` — the established "lazy per-content-hash cache living on `RenderContext`" shape now has three instances, reinforcing it as this crate's standard extension point for future imported-content types.
- `engine-assets` gained its first import path that does *no* format validation or transformation at import time (`import_font` just content-addresses raw bytes) — unlike `import_texture` (decode+re-encode) and `import_gltf` (parse+validate), a malformed font file will import successfully and only fail later, lazily, at first render (`RENDER_FONT_PARSE_ERROR`). Consistent with the "engine-assets stores bytes, engine-render understands formats" division this ADR establishes, but worth remembering if font-import-time validation ever seems worth adding.
- `games/sandbox`'s scenes now depend on a vendored, git-committed OFL font family beyond the engine's own bundled default — the first game-specific (not engine-bundled) binary asset checked into the repo outside the content-addressed store's usual glTF/image content.

## Revisit when

- A concrete need for world-space text (floating nameplates, damage numbers) appears — `Text` would need a `Transform`-based variant or billboard mode, a genuinely different rendering path from this one's fixed screen-space projection.
- A concrete need for panels, buttons, or any interactive UI element appears — this ADR intentionally covers text only; hit-testing/layout for interactive widgets is unbuilt.
- Text at a wide range of sizes in the same scene looks visibly soft — that's the trigger to build an SDF atlas instead of scaling the fixed-base-size raster atlas.
- A scene needs more than a couple of distinct fonts active simultaneously and the per-font-group draw-call count becomes a real (not theoretical) performance concern.
