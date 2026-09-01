# Development Roadmap

**This is a living document, and everything in it — including tier order — is a suggestion, not a spec.** Every phase and tier boundary below is an explicit checkpoint to ask "does this still make sense given what we now know?", not a commitment to build things in the order written. If real implementation reveals a better approach than what's written here, build the better approach and update this doc to match — don't do the written thing out of obligation to the document. See [Practice: keep re-evaluating](#practice-keep-re-evaluating) below for how that's meant to work day to day.

Locked decisions so far: Rust, 3D from day one, building from scratch on top of focused libraries (not wrapping an existing engine). See [ADR-0001](docs/decisions/0001-rust-3d-from-scratch.md). Full original architecture reasoning: [research/00-synthesis-and-recommendations.md](research/00-synthesis-and-recommendations.md).

---

## Ground rules that apply to every phase

1. **Every new capability gets a CLI command (with a `--format json` mode) before or alongside anything else.** If a feature can only be exercised through ad hoc test code or a debugger, it isn't done yet.
2. **Headless is the default, not a flag.** A window is a consumer of the headless path, never a separate code path.
3. **Determinism is enforced from Phase 0**, not bolted on later: fixed timestep, one explicitly-threaded seeded RNG, stable iteration order. Retrofitting determinism into a codebase that grew up without it is much more expensive than building it in from the first tick.
4. **Every bug found becomes a permanent regression test** (recorded input/seed + expected snapshot), per the testing discipline in [research/03, §5](research/03-design-principles-for-agent-native-engines.md#5-testing-and-determinism).
5. **A phase isn't done until its "definition of done" CLI commands work end-to-end**, not until the underlying code merely compiles.

---

## Workspace layout (as built)

Crate boundaries have held for 8 phases without needing rework — no longer the tentative sketch the original draft of this section was, but still not frozen: change the shape the moment a real need contradicts it.

```
weft/
  Cargo.toml                 # workspace root
  crates/
    engine-core/              # ECS integration (hecs), math re-exports (glam), fixed-timestep scheduler, seeded RNG, the Resources extension bag
    engine-scene/              # TOML scene format; ComponentRegistry/SystemRegistry-driven loader with no compile-time knowledge of game types
    engine-assets/             # content-addressed binary asset store; glTF + image import
    engine-render/             # wgpu renderer — offscreen PNG export and live windowed presentation over one shared pipeline
    engine-physics/            # rapier3d integration behind engine-native component/system types
    engine-script/             # mlua-backed Lua scripting for content-level logic, plus scene/script hot-reload
    engine-cli/                # the `engine` binary: run/test/inspect/replay/render/import/play, --watch
    engine-mcp/                 # thin MCP server (rmcp) wrapping engine-cli's operations as typed tools
  games/
    sandbox/                  # first real test game — physics playground, WASD, camera-follow, imported assets
  tools/
    asset-gen/                 # standalone headless-Blender scripts (ADR-0009) — outside the Cargo workspace on purpose
  docs/
    decisions/                 # ADRs — why a decision was made and what would change it
    roadmap/                   # completed-phase history, plus the forward-looking capability tiers
  research/                   # the original research/synthesis docs the early architecture was built on
```

---

## How this roadmap is organized

- [`docs/roadmap/completed-phases.md`](docs/roadmap/completed-phases.md) — Phases 0–8, done. A historical build log, like `docs/decisions/` — not a place to plan new work.
- Forward-looking work is organized into four priority tiers, seeded by a full capability audit of the engine (2026-09-01) against what a fully realized game — realistic graphics, audio, animation, UI, everything a shipped game needs — actually requires:
  - [Tier 1 — Foundational](docs/roadmap/tier-1-foundational.md)
  - [Tier 2 — Visual & gameplay realism](docs/roadmap/tier-2-visual-and-gameplay-realism.md)
  - [Tier 3 — Polish & feel](docs/roadmap/tier-3-polish-and-feel.md)
  - [Tier 4 — Ship readiness](docs/roadmap/tier-4-ship-readiness.md)
- **The tiers are a suggested order, not a queue.** Tier 1 is "foundational" in a literal sense — later tiers build on it, or retrofitting it after the fact is expensive — not because everything in it must happen strictly before anything in Tier 2. Pull whichever item, from whichever tier, a concrete need (usually `games/sandbox`) actually points to next. That's the same "let real usage demand the shape" discipline every phase in the completed history already followed.
- As tier items actually get built, record what happened in `docs/roadmap/completed-phases.md` as a new numbered phase (Phase 9 onward), the same way Phases 0–8 were recorded — and write an ADR in `docs/decisions/` for any decision worth remembering *why* it was made.

---

## Practice: keep re-evaluating

The user's explicit instruction for this project is to *not* treat any of this as fixed — constraints that only become visible during real implementation should actively change the plan.

- **Before starting any new item**, explicitly revisit: does the ECS choice still feel right? Is the scene text format's schema holding up, or has it needed awkward workarounds? Is `wgpu` giving the right level of control? Has anything in `research/` or in the tier files turned out to be wrong once tested against real code?
- **When a decision changes**, write a new ADR (or mark an existing one "superseded by") in `docs/decisions/` rather than silently drifting — the point isn't ceremony, it's leaving a trail so a future session (agent or human) understands *why* the architecture looks the way it does, not just what it currently looks like.
- **This file and the tier files under `docs/roadmap/` should be edited directly** as work completes, scope shifts, or priorities change — they are not a historical record; `docs/roadmap/completed-phases.md` and `docs/decisions/` are. Keep these describing the current plan, and let the history files carry how it got there.
