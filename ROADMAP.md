# Development Roadmap

**This is a living document.** It is a guide for getting started, not a spec to freeze against. Every phase boundary below is an explicit checkpoint to ask "does the architecture still make sense given what we now know?" — see [Practice: keep re-evaluating](#practice-keep-re-evaluating) at the bottom. When a decision is worth remembering *why* we made it, write an ADR in [`docs/decisions/`](docs/decisions/) (template there) instead of only editing this file — this file says what we're doing now, ADRs say why and what would change our mind.

Locked decisions so far: Rust, 3D from day one, building from scratch on top of focused libraries (not wrapping an existing engine). See [ADR-0001](docs/decisions/0001-rust-3d-from-scratch.md). Full architecture reasoning: [research/00-synthesis-and-recommendations.md](research/00-synthesis-and-recommendations.md).

---

## Ground rules that apply to every phase

1. **Every new capability gets a CLI command (with a `--format json` mode) before or alongside anything else.** If a feature can only be exercised through ad hoc test code or a debugger, it isn't done yet.
2. **Headless is the default, not a flag.** A window is a consumer of the headless path, never a separate code path.
3. **Determinism is enforced from Phase 0**, not bolted on later: fixed timestep, one explicitly-threaded seeded RNG, stable iteration order. Retrofitting determinism into a codebase that grew up without it is much more expensive than building it in from the first tick.
4. **Every bug found becomes a permanent regression test** (recorded input/seed + expected snapshot), per the testing discipline in [research/03, §5](research/03-design-principles-for-agent-native-engines.md#5-testing-and-determinism).
5. **A phase isn't done until its "definition of done" CLI commands work end-to-end**, not until the underlying code merely compiles.

---

## Proposed workspace layout

Subject to change once real code exists — this is a starting scaffold, not a commitment:

```
weft/
  Cargo.toml                 # workspace root
  crates/
    engine-core/             # ECS integration, math re-exports, deterministic scheduler, seeded RNG plumbing
    engine-scene/             # text scene/entity/asset-template DSL, parsing, loading into ECS world
    engine-assets/            # content-addressed binary asset store, glTF import, texture import
    engine-render/            # wgpu-based renderer, headless-first, offscreen target + windowed wrapper
    engine-physics/           # rapier3d integration behind our own component/system types
    engine-script/            # mlua integration for content-level scripting + hot-reload plumbing
    engine-cli/               # the `engine` binary: build/run/test/inspect/replay/render/export subcommands
    engine-mcp/                # thin MCP server wrapping engine-cli's operations as typed tools
  games/
    sandbox/                 # first real test project built with the engine, used as the integration testbed
  docs/
    decisions/                # ADRs
  research/                  # existing research + synthesis docs
```

Naming, crate boundaries, and even the `crates/` vs. flat-workspace layout are all up for revision once Phase 0 makes real usage patterns visible.

---

## Phase 0 — Deterministic core loop (no rendering yet) — **done** (2026-08-28)

**Goal**: prove the ECS + deterministic scheduler + CLI + JSON-inspection loop end-to-end before spending any time on graphics.

- Cargo workspace scaffold per above.
- `engine-core`: wrap an existing ECS crate (default choice: `hecs` — minimal, archetype-based, no imposed app/plugin model; see [research/03, §2](research/03-design-principles-for-agent-native-engines.md#2-ecs-architecture-over-deep-oop-inheritance)). Add a fixed-timestep scheduler on top and a single seeded RNG resource threaded explicitly through system calls (never `rand::thread_rng()` ambient access anywhere in engine or game code).
- `engine-cli`: skeleton binary with `engine test` wired to a trivial in-process simulation (e.g. a few entities with a `Position`/`Velocity` component pair and one system) — no scene file loading yet, just prove the loop and the CLI plumbing.
- `engine inspect --format json`: serialize full ECS world state (every entity's components) to JSON.
- `engine replay <recording>`: record an input+seed stream, re-run it deterministically, and dump state at any frame — validated with a test that runs the same recording twice and asserts byte-identical JSON output both times.

**Definition of done**: `engine test` runs a scripted scenario twice with the same seed and produces byte-identical `engine inspect` JSON output both times; a deliberately-introduced nondeterminism (e.g. an ambient RNG call) is caught by this test failing.

**Crates to evaluate**: `hecs` (default), `glam` (math), `clap` (CLI), `serde`/`serde_json` (state dumps), a seeded PRNG (`rand` with an explicit `SmallRng`/`ChaCha8Rng` instance, not the global generator).

**Implementation notes**: built as `crates/engine-core` + `crates/engine-cli` only — `engine-scene`/`engine-render`/`engine-assets`/`engine-physics`/`engine-script`/`engine-mcp` and `games/sandbox` remain unscaffolded until their own phases need them. Went with `ChaCha8Rng` over `SmallRng` and codified a hecs-iteration-order sorting rule; see [ADR-0002](docs/decisions/0002-deterministic-rng-and-hecs-iteration-order.md). `cargo test --workspace` (16 tests) is green, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all -- --check` both pass.

---

## Phase 1 — Text scene format — **done** (2026-08-28)

**Goal**: move from hardcoded test scenarios to the text DSL described in [research/03, §1](research/03-design-principles-for-agent-native-engines.md#1-scene-and-asset-data-formats-text-diffable-git-mergeable).

- Design the entity/prefab/system text format (starting point: TOML via `serde`/`toml`, one file per entity template, name-based references, no engine-recomputed counters stored inline — re-evaluate against a hand-rolled DSL only if TOML's expressiveness genuinely becomes a blocker, not preemptively).
- `engine-scene`: parse the format, load it into an `engine-core` world.
- `engine run <scene> --headless`: load a scene file, run N ticks, exit.
- Extend `engine inspect` to work against a loaded scene file, not just the Phase 0 hardcoded scenario.

**Definition of done**: a hand-written scene text file with a handful of entities loads, runs deterministically, and its state is inspectable/diffable exactly like Phase 0's hardcoded scenario. A round-trip test: edit one entity's starting values in the text file, confirm only the expected part of the JSON output changes.

**Implementation notes**: built as `crates/engine-scene` — a `SceneDef`/TOML parser plus a caller-supplied `ComponentRegistry`/`SystemRegistry` so the generic loader has no compile-time knowledge of `Position`/`Velocity`-style game components (mirrors the `ComponentDumper` fn-pointer pattern from Phase 0, in reverse). `engine-cli::registry` wires the existing `basic` scenario's component/system types into that registry rather than duplicating them; no `games/sandbox` crate yet (still deliberately deferred). Every scene-loaded entity gets an automatic `SceneName` component so `engine inspect` output is diffable by the name an author chose, not by hecs's internal entity id. `test`/`inspect`/`replay` were generalized around a `SimSource` enum (`Scenario(String) | Scene(PathBuf)`) so scene-file support reached every existing command, not just the new `run` subcommand; `Recording` files can now point at a `scene` path instead of a `scenario` name. See [ADR-0003](docs/decisions/0003-text-scene-format.md). `cargo test --workspace` (28 tests) is green, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all -- --check` both pass.

---

## Phase 2 — Headless-first 3D rendering — **done** (2026-08-28)

**Goal**: get pixels on screen (or into a buffer) without ever requiring a window.

- `engine-render`: wgpu-based renderer targeting an offscreen texture first; a windowed mode (via `winit`) is a thin wrapper that presents the same offscreen frames to a swapchain, not a separate rendering path.
- Minimal 3D pipeline: camera, a mesh (start with hardcoded cube/plane geometry — glTF import comes in Phase 3), a basic unlit or simple-lit material. Full PBR is not a Phase 2 goal.
- `engine render <scene> --to <file.png>`: headless render of a scene to a still image.
- Golden-image test harness: render a known scene, compare against a checked-in reference image with a defined tolerance.

**Definition of done**: `engine render` produces a correct, deterministic PNG for a simple scene with no window/display server present (verify this explicitly, e.g. in a container with no `DISPLAY` set); a golden-image regression test passes.

**Implementation notes**: built as `crates/engine-render` — a `wgpu` pipeline restricted to `Backends::VULKAN` so it runs against Mesa's `lavapipe` software rasterizer with no GPU or display server (this environment needed `mesa-vulkan-drivers` installed; see [ADR-0004](docs/decisions/0004-headless-rendering-backend.md)). `engine-core` gained its first first-class component, `Transform` (`glam::Vec3`/`Quat`, engine-core's first real use of `glam`); `engine-render` adds its own `Camera` (look-at target, not a rotation quaternion — easier to hand-author), `MeshRef` (hardcoded `Cube`/`Plane`), and `Material` (flat color) components, registered into `engine-cli`'s existing `ComponentRegistry` alongside Phase 1's `Position`/`Velocity`. Shading is simple-lit (one hardcoded directional light, Lambertian + ambient) per-face on the cube, not unlit, to prove the normal pipeline ahead of Phase 3's real assets. The golden-image test (`crates/engine-cli/tests/render.rs`) compares against a checked-in reference PNG with a fixed per-channel tolerance, not byte equality — cross-machine Mesa version drift is an accepted non-goal, the same shape of caveat ADR-0002 and the roadmap's own Phase 6 note already accept for `rapier3d`. `cargo test --workspace` (36 tests) is green, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all -- --check` both pass.

**Re-evaluation checkpoint, done**: `wgpu`'s abstraction level held up fine for a minimal offscreen pipeline — the friction was entirely API churn between tutorial-era `wgpu` and the current `30.x` release (renamed types like `TexelCopyBufferInfo`, `PollType`, `immediate_size`), not the abstraction itself being wrong. No reason found to go closer to native graphics APIs; revisit only if a concrete need (e.g. multi-threaded command recording, bindless resources) actually hits `wgpu`'s ceiling.

---

## Phase 3 — Asset pipeline

**Goal**: real content instead of hardcoded geometry.

- `engine-assets`: content-addressed binary asset store (hash-named files, referenced by hash/path from scene text files — mirrors glTF's JSON/binary split per [research/03, §1](research/03-design-principles-for-agent-native-engines.md#1-scene-and-asset-data-formats-text-diffable-git-mergeable)).
- glTF import (via the `gltf` crate) as the standard interchange format for meshes/materials/animations — both because it's the industry-standard text+binary split and because it's what generative 3D asset APIs (Meshy, Tripo3D) export natively.
- Texture import via the `image` crate.
- `engine import <file.gltf>`: converts an external asset into the engine's content-addressed store and emits a scene-text-file reference block ready to paste into a scene.

**Definition of done**: a glTF file (e.g. a free sample asset) imports cleanly and renders correctly via `engine render`; re-importing the same file produces the same content hash (no spurious diff/churn).

---

## Phase 4 — Scripting & hot-reload

**Goal**: close the edit-run-observe loop enough that iteration doesn't require a full rebuild for every change.

- Data/scene text files: hot-reload on save via a file watcher (`notify` crate) — no engine restart required to see a scene-file edit take effect.
- Content-level scripting: embed Lua via `mlua` for dialogue/quest/tuning logic, hot-reloadable with no rebuild, per [research/03, §6](research/03-design-principles-for-agent-native-engines.md#6-scriptingextension-layer). Gameplay *systems* remain native Rust — Lua is for low-stakes content, not core logic.
- Native hot-reload for Rust systems is the highest-risk item in this phase (Rust doesn't make this easy). Timebox an evaluation of existing approaches (e.g. `hot-lib-reloader`-style dylib reloading) before committing engineering time; it's acceptable to ship Phase 4 with data/scene hot-reload and Lua hot-reload only, deferring native hot-reload if it proves to be a rabbit hole.

**Definition of done**: editing a scene text file or a Lua script while `engine run` is active takes effect without restarting the process; a deliberate mistake in a Lua script produces a structured, file/line-precise error rather than a silent no-op or an opaque panic.

---

## Phase 5 — Agent tool interface (CLI polish + MCP)

**Goal**: the actual point of this whole project — make the engine a good set of tools for an agent to use.

- Diagnostics pass across every CLI command built so far: every failure path gets a stable error code, the offending file/entity, and a plain-language description — audit against the anti-pattern called out in [research/03, §7](research/03-design-principles-for-agent-native-engines.md#7-agent-tool-interface-design) (Unreal Blueprints silently no-op-ing on a bad request).
- `engine-mcp`: thin MCP server (Rust SDK: `rmcp`) exposing `build`/`run`/`test`/`inspect`/`replay`/`render`/`import` as typed tools, each a direct wrapper over the corresponding CLI/library call — no MCP-only logic.
- Write agent-facing documentation for the engine itself (an `AGENTS.md`/`CLAUDE.md` at the project root) describing the CLI contract, once it's stable enough to document.

**Definition of done**: an agent (this one) can build a trivial scene, run it, inspect its state, and diagnose a deliberately introduced bug using only the CLI/MCP surface — no reading engine source code required to accomplish that loop.

---

## Phase 6 — Physics & fuller gameplay substrate

- `engine-physics`: integrate `rapier3d` behind engine-native component/system types (don't leak rapier's own types into scene files/gameplay code any more than necessary).
- Flag explicitly: `rapier3d`'s floating-point determinism is reliable across runs on the same machine/build but is **not** guaranteed bit-identical across different hardware/compiler versions. This is fine for the single-machine agent-loop use case now; revisit if/when multiplayer lockstep networking (Phase 8+) makes cross-machine determinism a hard requirement.
- Broaden the ECS component/system library toward an actual gameplay substrate (input handling, simple AI/behavior primitives, animation) as real test games in `games/` demand it — resist building speculative systems ahead of a concrete need.

---

## Phase 7 — Generative content integrations

- Wire Meshy and/or Tripo3D's REST APIs into `engine import` as an alternative asset source: agent describes an asset in natural language, the API call produces a glTF/mesh+texture result, which flows through the same Phase 3 import path as any hand-authored asset.
- Evaluate NVIDIA ACE-style patterns for LLM-driven NPC dialogue/behavior as an optional runtime layer, per [research/02](research/02-world-models-and-generative-tooling.md#5-npcdialogue-agent-patterns-nvidia-ace-are-mature-and-shippable). Not a core-engine dependency — an opt-in gameplay-layer feature.
- Explicitly deferred, not scheduled: any neural-world-model-based rendering layer (Genie/Oasis-style). Revisit per the "revisit when" criteria in [research/00](research/00-synthesis-and-recommendations.md#layer-5--contentasset-pipeline-where-generative-ai-actually-plugs-in) — no sooner than a public, stable, controllable API exists for one of these models.

---

## Not yet scheduled (deliberately)

These are real future needs but don't have a phase number yet because scoping them now would be speculative ahead of the constraints Phases 0–7 will surface: audio, networking/multiplayer, a packaging/export pipeline for shipping builds, animation blending/state machines beyond the basics, and any optional GUI layer (which, per [ADR-0001](docs/decisions/0001-rust-3d-from-scratch.md) and the engine's core thesis, must be built strictly on top of the same CLI/API surface, never as a capability's only entry point).

---

## Practice: keep re-evaluating

The user's explicit instruction for this project is to *not* treat any of this as fixed — constraints that only become visible during real implementation should actively change the plan. Concretely:

- **At the end of every phase above**, before starting the next one, explicitly revisit: does the ECS choice still feel right? Is the scene text format's schema holding up, or has it needed awkward workarounds? Is `wgpu` giving the right level of control? Has anything from `research/` turned out to be wrong once tested against real code?
- **When a decision changes**, write a new ADR (or mark an existing one "superseded by") in `docs/decisions/` rather than silently drifting — the point isn't ceremony, it's leaving a trail so a future session (agent or human) understands *why* the architecture looks the way it does, not just what it currently looks like.
- **This file should be edited directly** as phases complete, scope shifts, or new phases get added — it is not a historical record, `docs/decisions/` is. Keep `ROADMAP.md` describing the current plan, and let the ADRs carry the history of how it got there.
