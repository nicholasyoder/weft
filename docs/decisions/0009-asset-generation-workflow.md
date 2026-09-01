# ADR-0009: Asset generation stays outside the engine, via two complementary Blender-based workflows

- **Status**: accepted
- **Date**: 2026-09-01

## Context

[ROADMAP.md](../../ROADMAP.md) Phase 7 originally proposed wiring Meshy/Tripo3D's paid REST APIs directly into `engine import`. The user rejected that shape outright: asset generation is a different concern from the engine itself, and the engine should carry no hardcoded external API dependency. Separately, the user wants human-authored art to work identically to any generated content — nothing should assume everything comes from an AI pipeline.

Phase 3 ([ADR-0005](0005-asset-pipeline.md)) already built `engine import <file>` as fully source-agnostic: it content-addresses any glTF/image file regardless of where it came from. This ADR is about what produces that file, which is deliberately *not* an engine concern — but it's still worth a decision, since "just don't hardcode it" leaves open what to actually recommend and prototype.

Two rounds of research were done here. The first evaluated options against this development sandbox's own constraints (no GPU, no Blender installed) and concluded local ML models (TripoSR, Hunyuan3D-2, TRELLIS) were unusable here and that `blender-mcp`'s requirement of a live, interactive Blender GUI session conflicted with Weft's headless-first posture. The user corrected this framing directly: Weft is a game engine for other developers to build games with, not just for this sandbox — those developers will have Blender and real GPUs, and a live-GUI-based agent workflow is completely normal for content *authoring* (as opposed to the engine's own runtime, which does need to be headless-first). Re-evaluating against that actual target audience changes the recommendation.

Confirmed by reading the `blender-mcp` (ahujasid) repo directly: it requires a live interactive Blender GUI (no headless/background-mode support), bridges via a socket addon, and exposes `execute_blender_code` (arbitrary unsandboxed Python execution — flagged as a known risk in the project's own docs and issue #207). Its content/generation integrations — Poly Haven (free, no key), Poly Pizza (free, API key), Sketchfab (free, API key), and Hyper3D Rodin / Hunyuan3D (paid, API key) — are each independently toggleable in the addon's own UI, not bundled or forced.

## Decision

Asset generation for Weft is a developer-side workflow, not an engine feature, and it is documented (not built) as two complementary patterns, both of which only ever produce plain glTF/image files consumed by the existing `engine import` command:

1. **Interactive creative authoring**: an agent driving a live, GUI Blender session via an MCP bridge (e.g. `blender-mcp`). This is the primary recommended path for one-off, creative, or visually-iterated assets — a real developer already has Blender open while working, so requiring a live GUI session is not a limitation the way it would be for the engine's own runtime. Any paid or GPU-heavy integration (Hyper3D Rodin, a locally-hosted Hunyuan3D-2/TRELLIS, Sketchfab) is the individual developer's own opt-in tool choice, configured in their own Blender addon and their own agent's MCP config — never a Weft dependency.
2. **Headless procedural scripting**: `blender --background --python script.py`, agent-authored `bpy`/geometry-nodes scripts, no live process, no ML model. This is the path for deterministic, reproducible, or batch-generated content (prop kits, terrain variants, level geometry from rules) — in the spirit of Infinigen's "math rules only" approach — and is the better match for Weft's own deterministic, text-diffable ethos when that fits the content being made.

Neither lives in `crates/engine-*` or the Rust workspace. Both are external to Weft's own dependency graph; the only contract is "produces a file `engine import` can consume."

## Alternatives considered

- **Hardcode Meshy/Tripo3D into `engine import` (the original Phase 7 roadmap text).** Rejected per explicit user direction: couples the engine to a specific paid vendor, and doesn't help developers who want to author assets by hand or use a different generation tool.
- **`blender-mcp` only, no headless path.** Rejected as too narrow: batch/programmatic content (e.g. many terrain variants, a prop kit) doesn't need or benefit from an interactive session, and a purely interactive workflow can't be driven unattended (CI, regeneration-on-scene-change).
- **Headless scripting only, no interactive/MCP path.** This was the first-round recommendation, made when evaluating options only against this sandbox's own missing GPU/Blender. Rejected on reconsideration: it throws away real capability (visual iteration, Blender's full toolset, free CC0 library search through Poly Haven/Poly Pizza) that a real developer's normal working setup makes essentially free to use.
- **Defer local ML models (TripoSR, Hunyuan3D-2, TRELLIS) as out of scope entirely.** Not rejected, narrowed instead: still not something Weft integrates directly, but no longer dismissed as unusable — a developer with a real GPU can run them locally (standalone, or via `blender-mcp`'s optional Hunyuan3D toggle) entirely at their own discretion.

## Consequences

- The engine gains zero new code, crates, or dependencies from this decision — `engine import`'s existing source-agnostic design already covers it, the fourth ADR in a row (0003, 0005, 0007-adjacent) to confirm that design scales without new mechanism.
- Documentation (this ADR, and a future `AGENTS.md`/docs note once a concrete workflow is actually exercised) is the deliverable, not a `tools/asset-gen/` framework built ahead of need — matching the roadmap's own repeated "don't build gameplay/content substrate speculatively" discipline.
- The `execute_blender_code` arbitrary-code-execution caveat should be called out to users of any future setup docs, not silently glossed over — it's the same trust boundary as giving an agent shell access, not a Weft-specific risk, but worth naming.
- The first real prototype of either pattern is deferred to whenever `games/sandbox` (the first real test game) actually needs a concrete asset — consistent with every prior phase's "let real usage demand the shape" posture.

## Revisit when

- ~~`games/sandbox` needs its first real 3D asset~~ — **done** (2026-09-01): the headless Blender-scripting pattern was prototyped end-to-end (`tools/asset-gen/generate_crate.py` → `engine import`, no engine changes needed — see ROADMAP.md's Phase 8 follow-up notes for the full account and the real environment friction hit along the way). Extended the same day: the script's `--color`/`--bevel-width` parameters let it deterministically produce a second, distinct crate variant (`pillar_steel` in `playground.toml`) — the "prop kit" case this ADR names as headless scripting's natural fit, now exercised rather than just described. The interactive `blender-mcp` pattern remains unprototyped — this sandbox has no Blender GUI/display to exercise it, and it's the developer-workstation path this ADR always expected to be tried on a real machine, not here.
- A developer/user reports friction with either pattern (e.g. `blender-mcp`'s live-session requirement proves annoying in practice, or headless scripts prove too limited for the content actually needed) — revisit the split rather than assuming it holds.
- A local ML model's GPU/VRAM floor drops enough to run on modest consumer hardware without a live Blender session at all — that could become a third first-class pattern rather than an opt-in toggle inside `blender-mcp`.
