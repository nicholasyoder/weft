# Weft

A game engine built from the ground up to be developed and operated primarily by AI coding agents — no GUI editor required for any core capability, and the engine's own development process is itself AI-driven.

## Why

Existing engines (Unity, Unreal, and to a lesser extent Godot) were designed around a human sitting at a monitor, dragging nodes in a viewport and eyeballing a running game. That model is a poor fit for an AI coding agent, which has a filesystem, a shell, and structured tool output — not eyes and a mouse. Binary scene formats, GUI-only operations, and unstructured error output force an agent to build an entire simulated human just to do what the engine assumes a person will do directly.

This project starts from the opposite assumption: every operation should be doable with a text editor, a compiler, and a CLI, and every result should come back as structured, parseable feedback.

## Status

Planning phase. No code yet. Language, scope, and build-vs-wrap decisions are locked (Rust, 3D from day one, built from scratch on focused libraries — see [ADR-0001](docs/decisions/0001-rust-3d-from-scratch.md)); the phased build plan lives in [ROADMAP.md](ROADMAP.md). **The roadmap is explicitly a living document** — every phase boundary is a checkpoint to reconsider the architecture as real constraints show up, not a fixed spec.

## Where to start

- [ROADMAP.md](ROADMAP.md) — the phased development plan, current status, and the practice for revisiting decisions as we go.
- [`docs/decisions/`](docs/decisions/) — ADRs recording why a decision was made and what would change it.
- [`research/`](research/) — the original research and architecture proposal that the roadmap is built on.

## Research index

- [00 — Synthesis & architecture recommendation](research/00-synthesis-and-recommendations.md)
- [01 — Prior art: engines, frameworks, and tooling](research/01-prior-art-engines-tooling.md)
- [02 — Neural world models & generative tooling](research/02-world-models-and-generative-tooling.md)
- [03 — Design principles for agent-native engines](research/03-design-principles-for-agent-native-engines.md)

## Working thesis (short version)

Keep a deterministic, text-represented, ECS-based simulation core as the single source of truth. Expose it through a small, stable, headless-first CLI (mirrored by a thin MCP server) with structured diagnostics on every failure path. Prefer JSON world-state dumps and deterministic replay over screenshots for verifying gameplay logic; reserve rendering/vision-based feedback for genuinely visual questions. Treat generative AI (asset generation, NPC dialogue, eventually world-model-based rendering) as an optional layer on top of that core, never as the core itself.

Full reasoning and trade-offs in [research/00-synthesis-and-recommendations.md](research/00-synthesis-and-recommendations.md).
