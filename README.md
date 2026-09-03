# Weft

A game engine built from the ground up to be developed and operated primarily by AI coding agents — no GUI editor required for any core capability, and the engine's own development process is itself AI-driven.

## Why

Existing engines (Unity, Unreal, and to a lesser extent Godot) were designed around a human sitting at a monitor, dragging nodes in a viewport and eyeballing a running game. That model is a poor fit for an AI coding agent, which has a filesystem, a shell, and structured tool output — not eyes and a mouse. Binary scene formats, GUI-only operations, and unstructured error output force an agent to build an entire simulated human just to do what the engine assumes a person will do directly.

This project starts from the opposite assumption: every operation should be doable with a text editor, a compiler, and a CLI, and every result should come back as structured, parseable feedback.

## Status

Phases 0–14 are done: a deterministic ECS core (`engine-core`), a text scene format (`engine-scene`), headless-first `wgpu` rendering with live windowed play, text/UI rendering, and GPU skeletal animation (`engine-render`), a content-addressed glTF/image/font/audio asset pipeline (`engine-assets`), Lua scripting with hot-reload and a live/scripted-input API (`engine-script`), `rapier3d` physics (`engine-physics`), a deterministic skeletal-animation sampler (`engine-anim`), an audio playback + deterministic offline-mixdown layer (`engine-audio`), a CLI and MCP server exposing all of it (`engine-cli`, `engine-mcp`), and a first real test game (`games/sandbox` — WASD rolls a ball around a walled physics playground, collects scripted pickups with sound effects, and shows a HUD in a custom imported font). The full build log is [`docs/roadmap/completed-phases.md`](docs/roadmap/completed-phases.md). **Tier 1 of the forward-looking roadmap (below) is fully closed** as of Phase 14.

A 2026-09-01 capability audit weighed the engine against what a fully realized game — realistic graphics, audio, animation, UI, everything a shipped game needs — actually requires; Tier 1's items (text/UI, audio, animation, expanded scripting, generalized input) are now done. What's still ahead: PBR materials/normal mapping/multi-light/shadows, a physics gameplay substrate (raycasts, triggers, character controller), rendering polish (AA, transparency, post-processing), and ship-readiness concerns (packaging, additional input devices, CI, networking) — organized into the remaining tiers in [ROADMAP.md](ROADMAP.md). Separately, [`docs/roadmap/known-issues.md`](docs/roadmap/known-issues.md) tracks places the code doesn't (yet) do what its own docs/tests/design already claim — architectural debt and process/testing gaps, kept trimmed to what's currently open. **The roadmap — phases, tiers, and known issues alike — is explicitly a living document**, and the tiers are a suggested order, not a queue: every boundary is a checkpoint to reconsider the plan as real constraints show up, never a fixed spec to build against on faith.

Language, scope, and build-vs-wrap decisions are locked (Rust, 3D from day one, built from scratch on focused libraries — see [ADR-0001](docs/decisions/0001-rust-3d-from-scratch.md)); RNG-algorithm and ECS-iteration-order determinism policy are in [ADR-0002](docs/decisions/0002-deterministic-rng-and-hecs-iteration-order.md).

## Where to start

- [ROADMAP.md](ROADMAP.md) — ground rules, workspace layout, and how the roadmap is organized: completed-phase history plus the four forward-looking capability tiers, all under [`docs/roadmap/`](docs/roadmap/).
- [`docs/decisions/`](docs/decisions/) — ADRs recording why a decision was made and what would change it.
- [`research/`](research/) — the original research and architecture proposal the early roadmap was built on.

## Research index

- [00 — Synthesis & architecture recommendation](research/00-synthesis-and-recommendations.md)
- [01 — Prior art: engines, frameworks, and tooling](research/01-prior-art-engines-tooling.md)
- [02 — Neural world models & generative tooling](research/02-world-models-and-generative-tooling.md)
- [03 — Design principles for agent-native engines](research/03-design-principles-for-agent-native-engines.md)

## Working thesis (short version)

Keep a deterministic, text-represented, ECS-based simulation core as the single source of truth. Expose it through a small, stable, headless-first CLI (mirrored by a thin MCP server) with structured diagnostics on every failure path. Prefer JSON world-state dumps and deterministic replay over screenshots for verifying gameplay logic; reserve rendering/vision-based feedback for genuinely visual questions. Treat generative AI (asset generation, NPC dialogue, eventually world-model-based rendering) as an optional layer on top of that core, never as the core itself.

Full reasoning and trade-offs in [research/00-synthesis-and-recommendations.md](research/00-synthesis-and-recommendations.md).
