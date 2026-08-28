# Synthesis: Architecture Recommendation for an Agent-Native Game Engine

This document synthesizes the three research passes in this directory ([01-prior-art](01-prior-art-engines-tooling.md), [02-world-models](02-world-models-and-generative-tooling.md), [03-design-principles](03-design-principles-for-agent-native-engines.md)) into one opinionated architecture proposal. Where the research points in a clear direction, this document makes a decision rather than presenting options.

---

## The convergent finding

All three research passes — surveying existing engines, surveying the neural-world-model space, and reasoning from first principles about agent ergonomics — landed on the **same shape** independently:

> Keep a deterministic, inspectable, text-represented simulation core as the source of truth. Give it a small, stable, CLI/protocol-shaped control surface. Treat anything generative (rendering, assets, NPC dialogue) as an optional layer *on top of* that core, never as the core itself.

This is Roblox's "Reality Hybrid Architecture," it's what Bevy Agent, Summer Engine, and every serious RL environment already do, and it's the opposite of what pure neural world models (Genie, Oasis, GameNGen) currently offer. That convergence is the strongest signal in this whole research pass — it means the architecture below isn't a novel bet, it's an aggressive, from-scratch commitment to a pattern that's already validated piecemeal across a dozen unrelated projects.

It also means the honest whitepace isn't "invent a new paradigm" — it's **"build the thing several people are converging on, but as a first-class ground-up design instead of a bolted-on layer,"** the same bet SPARQ is making with venture funding, and the same gap Summer Engine/OpenGame are currently filling by wrapping Godot/web engines instead of building fresh.

---

## Recommended architecture

### Layer 0 — Core: ECS + deterministic scheduler
- Data-oriented ECS (components = plain serializable structs, systems = plain functions over queries). No inheritance, no macro-heavy reflection magic.
- Fixed-timestep simulation loop, one explicitly-threaded seeded RNG (never ambient/global), stable/deterministic system-iteration order.
- **Build vs. borrow**: use an existing, boring, well-tested ECS crate as a *library* (e.g. `hecs`, `flecs`, or `bevy_ecs` standalone — all are usable without pulling in a full engine/editor) rather than writing a new one. The differentiator of this project is everything above and around the ECS, not the ECS algorithm itself.

### Layer 1 — Simulation & world state
- Canonical scene/entity/world data lives in a text DSL (TOML-flavored), one file per entity template/prefab/system, name-based references (never GUIDs), no engine-recomputed counters inline. Mirrors glTF's split: meaning lives in text, bulk payload (mesh/texture/audio bytes) lives in separate content-addressed binary files.
- `engine inspect --format json` dumps full world state on demand. This is the primary feedback channel for an agent — cheaper, more precise, and more reliable than a screenshot, and it should exist before rendering does.
- `engine replay <recording>` re-runs a recorded input+seed stream deterministically, with state dumps at any frame. Every bug an agent fixes becomes a permanent recording in the test suite.

### Layer 2 — Rendering (headless-first)
- Renderer (wgpu-based, cross-platform) must run fully offscreen with no window/display server — this is not a mode, it's the default. A window is one more consumer of offscreen frames, not a separate code path.
- Screenshot/video capture + image diffing for visual regression is a *second-tier* tool behind JSON state assertions, used only for genuinely visual questions (layout, readability, shader correctness). Route diffs through a short VLM-generated natural-language description rather than raw pixels, per the design-principles research (§4) and GameDevBench's finding that visual feedback measurably helps once state-based checks are exhausted.

### Layer 3 — Extension / scripting
- Primary authoring language for engine and gameplay *systems*: a statically-typed compiled language with hot-reload (Rust is the natural choice given the ECS ecosystem above). The compiler's diagnostics are the single highest-leverage self-correction tool an agent gets for free — this is a deliberate, evidence-backed bet (§6 of the design-principles doc), not a default.
- Secondary embedded scripting layer (Lua via `mlua`, hot-reloadable with no rebuild) for high-churn, low-risk content: dialogue, quest logic, per-encounter tuning. Never the substrate for core systems.

### Layer 4 — Agent tool interface
- **CLI first.** `engine build|run|test|import|inspect|replay|export`, every command headless-by-default, structured (JSON) output mode, non-zero exit codes on failure, and — critically — diagnostics with a stable error code, offending file/entity, and a plain-language description on *every* failure path. A silent no-op on a bad request (Unreal Blueprints' worst failure mode) is treated as a bug.
- **MCP server as a thin wrapper**, not a parallel implementation — every MCP tool call maps 1:1 onto a CLI subcommand/library call, so the engine is never MCP-only. This matches what Unity, Godot, and Unreal have each independently converged on, and it's the most directly transferable, already-proven piece of prior art in this whole research pass.

### Layer 5 — Content/asset pipeline (where generative AI actually plugs in)
- 3D/texture asset generation (Meshy, Tripo3D — both have documented, agent-callable REST APIs today) as a first-class import path: agent describes an asset, calls the API, the result lands as a normal content-addressed binary asset referenced from the text scene DSL like any hand-authored asset.
- NPC dialogue/behavior via LLM calls at runtime (NVIDIA ACE's pattern) is a legitimate, already-mature use of generative AI *inside* the engine, orthogonal to the "is the engine itself neural" question.
- Explicitly **not** pursuing a neural-world-model runtime (Genie/Oasis-style) as the simulation core. Per research pass 02: no public control API exists for any of these today, none offer determinism/object-permanence/multiplayer-consistency, and the field's own credible commentary says augmentation (generative rendering layer on a deterministic core) is the right pattern, not replacement. Revisit this in 12–18 months, not now.

### Layer 6 — Version control & multi-agent workflows
- One file per entity/prefab/system, never one giant scene file. Stable name-based references. This is what makes it plausible for several agents (or several sequential agent sessions) to work on different features without touching the same file, let alone the same line — treated as a hard design constraint, not an afterthought.

---

## What this buys, concretely, for an agent loop

1. Agent edits a `.rs` system file or a text scene file directly with normal file tools — no GUI proxy needed.
2. `engine build` / `engine test` gives compiler diagnostics or deterministic-replay failures, both structured and precise.
3. `engine run <scene> --headless` + `engine inspect --format json` lets the agent assert exact facts about world state without a screenshot.
4. Only when a question is inherently visual does the agent reach for `engine render --to png` + an image/VLM diff.
5. Every bug fixed leaves behind a recorded regression test (input stream + seed + expected snapshot), so the suite grows automatically as a byproduct of normal agent work.
6. An MCP server exposes all of the above as typed tools for agent runtimes that prefer structured tool-calls over shelling out, with zero duplicated logic versus the CLI.

## Explicit non-goals (for now)

- No GUI editor as a required interface for any capability, ever. An optional GUI, if built later, must be a strictly additive layer on top of the same CLI/API surface — never a capability's only entry point.
- No dependency on neural world models as a runtime/simulation source of truth (see Layer 5).
- No custom ECS implementation — not a differentiator, adds risk for no agent-ergonomics benefit over an existing boring library.
- No commitment (yet) to a specific target genre (2D vs. 3D, single-player vs. multiplayer) — the architecture above is intentionally genre-agnostic; scope narrowing is a product decision, not an engine-architecture one.

## Open decisions worth a deliberate choice before writing code

These aren't blockers to continuing research/design, but they're consequential enough to decide explicitly rather than drift into:

- **Primary language**: Rust is the strong default given the ECS ecosystem (Bevy/EnTT/Flecs-adjacent), wgpu, and the compiler-diagnostics argument in §6 of the design-principles doc — but it's worth confirming against the team's/agent's actual proficiency and the value of tapping the existing Rust game-dev crate ecosystem.
- **2D vs. 3D scope for v0**: every piece of prior art that shipped fastest (PICO-8, LÖVE, Rosebud, OpenGame) started 2D/web-scoped. A 2D-first slice would validate the whole CLI/inspect/replay/MCP loop far faster than a 3D renderer would.
- **Whether the fastest path to a first working loop is truly "from scratch,"** or whether prototyping the exact same agent-facing CLI/MCP/inspect/replay contract on top of an already-headless, already-text-based substrate (Godot without its editor, or `bevy_ecs` directly) is a faster way to prove the design before committing to a bespoke renderer/ECS/asset pipeline. Section 1 of the prior-art research flags this explicitly as a live, unresolved trade-off (Summer Engine/OpenGame both chose "wrap," not "build fresh").
