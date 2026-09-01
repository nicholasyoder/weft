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

**Implementation notes**: built as `crates/engine-assets` — a content-addressed store checked into git (`assets/<hash[0:2]>/<hash>`, git-object-store-style) plus glTF import via the `gltf` crate and texture import via `image` (see [ADR-0005](docs/decisions/0005-asset-pipeline.md)). Import scope is deliberately narrow: one mesh/one primitive per glTF file (structured `ASSET_GLTF_UNSUPPORTED` error otherwise, same principle as Phase 2's single-camera requirement), base-color material only, no animation. A node's accumulated world transform is baked into the stored mesh at import time, which mattered immediately — both Khronos sample models used for fixtures carry a legacy Z-up-to-Y-up correction matrix on a wrapper node rather than the mesh node itself. `engine-render`'s `MeshKind` gained an `Asset(String)` variant and `Material` gained an optional `texture` field, both additive (no existing scene file needed changes); the shader now always samples a texture (a shared 1×1 white texture when a material has none), avoiding a branch for the untextured case. New `engine import <file>` subcommand (glTF or a loose image file) emits a pasteable scene-text-file fragment. `cargo test --workspace` (51 tests) + clippy + fmt all pass, including a CLI test that re-runs `engine import` against the same file and asserts byte-identical output and zero new files in the asset store — a direct proof of the DoD's "no spurious diff/churn."

**Re-evaluation checkpoint, done**: the registry design from ADR-0003 needed zero new mechanism to carry the new component fields (`MeshKind::Asset`, `Material.texture`) through scene files — they flow through the existing opaque `serde_json::Value` load/dump path untouched, the second phase in a row confirming that design scales. The main open question deferred rather than answered here is multi-part assets (more than one mesh/primitive per file); revisit once a real asset actually needs it rather than guessing at the shape now.

---

## Phase 4 — Scripting & hot-reload

**Goal**: close the edit-run-observe loop enough that iteration doesn't require a full rebuild for every change.

- Data/scene text files: hot-reload on save via a file watcher (`notify` crate) — no engine restart required to see a scene-file edit take effect.
- Content-level scripting: embed Lua via `mlua` for dialogue/quest/tuning logic, hot-reloadable with no rebuild, per [research/03, §6](research/03-design-principles-for-agent-native-engines.md#6-scriptingextension-layer). Gameplay *systems* remain native Rust — Lua is for low-stakes content, not core logic.
- Native hot-reload for Rust systems is the highest-risk item in this phase (Rust doesn't make this easy). Timebox an evaluation of existing approaches (e.g. `hot-lib-reloader`-style dylib reloading) before committing engineering time; it's acceptable to ship Phase 4 with data/scene hot-reload and Lua hot-reload only, deferring native hot-reload if it proves to be a rabbit hole.

**Definition of done**: editing a scene text file or a Lua script while `engine run` is active takes effect without restarting the process; a deliberate mistake in a Lua script produces a structured, file/line-precise error rather than a silent no-op or an opaque panic.

**Implementation notes**: built as `crates/engine-script` — an `mlua`-backed `ScriptHost` sandboxed with `StdLib::ALL_SAFE` (`math.random`/`math.randomseed` explicitly overridden to error, since `ALL_SAFE` alone still leaves ambient unseeded randomness available; no RNG binding is exposed to scripts at all yet), reachable only through `engine-cli`'s orchestration layer — `engine-core`/`engine-scene` gained no new types or fields (see [ADR-0006](docs/decisions/0006-scripting-and-hot-reload.md)). A new data-only `Script { path, function }` component is registered into the existing `ComponentRegistry` exactly like `Camera`/`Material`; `ScriptHost::dispatch` calls each scripted entity's function once per tick (in entity-id order, per ADR-0002) with its other components as a table (via `mlua`'s `serde` feature, reusing the same `ComponentDumper`/`ComponentLoader` Value-passthrough ADR-0003/0005 already validated), writing back whatever fields the function returns. `ComponentRegistry::loader`, previously `pub(crate)`, is now `pub` for this write-back path. Hot-reload (`engine run --scene --watch`) is one mechanism for both scene and script edits: any change to the scene file or a referenced `.lua` path (watched via `notify-debouncer-mini` on their parent directories, so editor temp-file+rename saves aren't missed) triggers a full rebuild of a fresh `Sim` and a full rerun of the same `--ticks` budget — there's no cheaper in-place patch path, since nothing in Weft yet has runtime state a rebuild would lose. Each run/reload prints one JSON (or human) event line; a `"watching"` event fires once the file watcher is actually armed, closing a real startup race (an edit made between the run finishing and `watch()` completing would otherwise be silently missed) that surfaced during testing. A reload error is always caught and reported (never allowed to kill the loop); the non-watch `run`/`test`/`replay` commands still hard-fail on a script error, same posture as any other bad scene. Native Rust hot-reload was evaluated and explicitly deferred (see ADR-0006) — no test game exists yet whose iteration loop it would actually improve. `cargo test --workspace` (58 tests) + clippy + fmt all pass, including new subprocess-driven tests (`crates/engine-cli/tests/watch.rs`, a new pattern for this repo since every prior CLI test was one-shot) that drive a live `engine run --watch` process through a scene edit, a script edit, and a broken-script edit, asserting the same process picks up each change and never crashes on the bad one.

**Re-evaluation checkpoint, done**: the ADR-0003/0005 Value-passthrough registry design scaled a third time with zero new generic mechanism — even bridging into a completely different language (Lua) via `mlua`'s `serde` feature needed no bespoke FFI bindings per component type, just the existing dump/load functions. The one real gap found only by writing the `--watch` integration tests (not by design review) was the watcher-arming race; fixed by adding an explicit `"watching"` readiness event, which is also a better CLI/agent contract than the race-prone alternative would have been. Open question deferred, not answered: whether scripts should ever get seeded-RNG access — revisit when a concrete script needs randomness, per ADR-0006.

---

## Phase 5 — Agent tool interface (CLI polish + MCP) — **done** (2026-08-31)

**Goal**: the actual point of this whole project — make the engine a good set of tools for an agent to use.

- Diagnostics pass across every CLI command built so far: every failure path gets a stable error code, the offending file/entity, and a plain-language description — audit against the anti-pattern called out in [research/03, §7](research/03-design-principles-for-agent-native-engines.md#7-agent-tool-interface-design) (Unreal Blueprints silently no-op-ing on a bad request).
- `engine-mcp`: thin MCP server (Rust SDK: `rmcp`) exposing `run`/`test`/`inspect`/`replay`/`render`/`import` as typed tools, each a direct wrapper over the corresponding CLI/library call — no MCP-only logic.
- Write agent-facing documentation for the engine itself (an `AGENTS.md`/`CLAUDE.md` at the project root) describing the CLI contract, once it's stable enough to document.

**Definition of done**: an agent (this one) can build a trivial scene, run it, inspect its state, and diagnose a deliberately introduced bug using only the CLI/MCP surface — no reading engine source code required to accomplish that loop.

**Implementation notes**: the diagnostics pass turned out to be mostly an *audit* rather than new error-handling work — every command already funneled failures through `engine_cli::diagnostics::CliError` (`{code, message, context}`, `Serialize`), and every downstream crate already exposed a stable `.code()` on its own `thiserror` error enum. No panic-on-bad-input paths were found. The audit's real, permanent output is `crates/engine-cli/tests/diagnostics.rs`, which closes the one gap that *was* real: no prior test asserted the `--format json` error envelope actually parses as JSON with a stable `error.code` (existing tests only checked stderr substrings), plus a handful of failure paths existing tests didn't reach yet (malformed scene TOML, a scene's dangling script reference outside `--watch`, an ambiguous recording, a two-camera scene). Before `engine-mcp` could wrap `render`/`import` without duplicating their logic, both got the same "one core fn in `engine_cli`'s public lib surface" treatment `test`/`inspect`/`run`/`replay` already had (`render_scene`, `import_asset`) — see [ADR-0007](docs/decisions/0007-cli-mcp-code-sharing.md), which also fixes the roadmap-text-vs-reality drift above: `build` was never a real CLI command (there are exactly six), so it's dropped from the tool list here and `engine-mcp` exposes the six real ones as `weft_run`/`weft_test`/`weft_inspect`/`weft_replay`/`weft_render`/`weft_import` (name-prefixed against ambiguity in a multi-server MCP client). Every tool returns `CallToolResult::structured`/`structured_error` — never a protocol-level `ErrorData` — so a domain failure reaches the calling agent as visible `{"error": {code, message, context}}` content, the identical envelope the CLI's own `--format json` prints; `crates/engine-mcp/tests/tools.rs` spawns the real `engine-mcp` binary as a subprocess (same posture as `engine-cli/tests/watch.rs`) and drives all six tools plus two deliberately bad inputs through `rmcp`'s own client transport — a direct regression test for this phase's own DoD. `AGENTS.md` (repo root) documents the resulting CLI/MCP contract for future sessions. `cargo test --workspace` (67 tests) + clippy + fmt all pass.

**Re-evaluation checkpoint, done**: the `run_and_dump`-style shared-core pattern (ADR-0003-era discipline, reaffirmed for `engine-mcp` by ADR-0007) held up for a fourth and fifth time with `render`/`import` — no new mechanism was needed, just moving existing logic one layer down. The one genuine judgment call was `import`'s CLI-vs-MCP divergence on `--out` (file write vs. always-return-the-fragment); documented rather than forced into false symmetry. No panics or unstructured failures turned up in the audit, which is itself a signal the Phase 0–4 discipline of routing every fallible operation through `CliError`/`thiserror` from the start (rather than retrofitting it) actually paid off — the "every bug found becomes a permanent regression test" rule from the ground rules above had already been followed closely enough that this phase found few bugs to convert.

---

## Phase 6 — Physics & fuller gameplay substrate — **partially done** (2026-08-31)

- `engine-physics`: integrate `rapier3d` behind engine-native component/system types (don't leak rapier's own types into scene files/gameplay code any more than necessary). — **done**
- Flag explicitly: `rapier3d`'s floating-point determinism is reliable across runs on the same machine/build but is **not** guaranteed bit-identical across different hardware/compiler versions. This is fine for the single-machine agent-loop use case now; revisit if/when multiplayer lockstep networking (Phase 8+) makes cross-machine determinism a hard requirement.
- Broaden the ECS component/system library toward an actual gameplay substrate (input handling, simple AI/behavior primitives, animation) as real test games in `games/` demand it — resist building speculative systems ahead of a concrete need. — **still open**, deliberately: no test game exists yet to demand any of this, same posture as every prior phase's "don't build ahead of a concrete need" deferrals.

**Implementation notes**: built as `crates/engine-physics`, wrapping `rapier3d` 0.35. The one real architectural gap this phase closed first: every system through Phase 5 was stateless between ticks (`SystemArgs` carried only `world`/`rng`/`tick`/`dt`), but rapier's body/collider sets need to persist across ticks within a `Sim`'s lifetime. `engine_core::Resources`, a small type-erased bag threaded through `SystemArgs` the same way `rng` already is, is the new extension point (see [ADR-0008](docs/decisions/0008-physics-and-scheduler-resources.md)); `engine-physics`'s `PhysicsState` (rapier's own `PhysicsWorld` convenience struct plus an entity↔handle map) is its first occupant, lazily initialized on first use so no special init phase was needed anywhere else. A genuine surprise: rapier3d 0.35's public `math::Vector`/`math::Rotation` types are `glam` types (via a `glamx` re-export), not `nalgebra` as the roadmap bullet above assumed when it was written — but a *different*, semver-incompatible `glam` release (0.33, pulled in transitively) than the workspace's own pinned `glam` 0.29, so `engine_physics::convert` bridges the two explicitly rather than either pulling in `nalgebra` or fighting a version conflict. `RigidBody`/`Collider` are engine-native components (`BodyType::{Dynamic, Fixed}` only — `Kinematic` was left out rather than half-wired, since nothing drives a body's pose externally yet; `ColliderShape::{Box, Sphere}` only, mirroring the single-camera/single-mesh scoping precedent from Phases 2–3), registered into `engine-cli`'s existing `ComponentRegistry`/`SystemRegistry` exactly like every prior phase's components — the scene-file format needed zero new mechanism to carry them, the fourth phase running in a row to confirm that. A new hardcoded `physics-demo` scenario and a matching `tests/fixtures/scenes/physics_demo.toml` (a dynamic ball falling onto a fixed ground plane) exercise both the scenario and scene-file paths; `crates/engine-cli/tests/physics.rs` and `engine-physics`'s own unit tests cover free-fall, resting-on-a-plane convergence, a fixed body never moving, and same-seed/same-tick determinism (the CLI-level test only claims determinism on this machine/build, per the roadmap's own pre-accepted rapier caveat above). No `AGENTS.md`/`engine-mcp` changes were needed — physics rides on the existing `run`/`test`/`inspect`/`replay` operations as new components + one new system, not a new verb. `cargo test --workspace` (84 tests) + clippy + fmt all pass.

**Re-evaluation checkpoint, done**: the `Resources` extension point (this phase's one new piece of core mechanism) worked on the first design — no rework needed once `PhysicsState` was built against it. The scene-format/registry design from ADR-0003 held for a fourth consecutive phase with zero new mechanism. The gameplay-substrate bullet stays open on purpose; revisit once a real `games/` project exists to demand specific pieces of it, rather than guessing at the shape now.

---

## Phase 7 — Generative content integrations — **revised** (2026-09-01)

- ~~Wire Meshy and/or Tripo3D's REST APIs into `engine import` as an alternative asset source~~ — **rejected, not deferred**: the user does not want any external asset-generation API hardcoded into the engine; asset generation is a developer-side workflow, not an engine feature. See [ADR-0009](docs/decisions/0009-asset-generation-workflow.md), which lands on two developer-facing patterns instead — an interactive agent+Blender-MCP workflow for creative/one-off assets, and headless Blender (`bpy`) scripting for deterministic/batch content — both producing plain glTF that flows through the existing, already-source-agnostic `engine import` (Phase 3). Neither pattern is built ahead of need; the first real prototype is deferred to whenever `games/sandbox` (Phase 8) needs a concrete asset. Human-authored art must work identically to generated content — this was never actually at risk given Phase 3's design, but is now an explicit constraint to hold onto.
- Evaluate NVIDIA ACE-style patterns for LLM-driven NPC dialogue/behavior as an optional runtime layer, per [research/02](research/02-world-models-and-generative-tooling.md#5-npcdialogue-agent-patterns-nvidia-ace-are-mature-and-shippable). Not a core-engine dependency — an opt-in gameplay-layer feature. Still open; no test game exists yet to demand it.
- Explicitly deferred, not scheduled: any neural-world-model-based rendering layer (Genie/Oasis-style). Revisit per the "revisit when" criteria in [research/00](research/00-synthesis-and-recommendations.md#layer-5--contentasset-pipeline-where-generative-ai-actually-plugs-in) — no sooner than a public, stable, controllable API exists for one of these models.

---

## Phase 8 — First test game (`games/sandbox`) — **partially done** (2026-09-01)

**Goal**: stand up the first real game built with Weft, so every deliberately-deferred "let real usage demand the shape" item (Phase 6's gameplay substrate, Phase 7's asset-generation prototype) has a concrete need to react to instead of being guessed at speculatively.

- Scaffold `games/sandbox` as a real crate/project using the engine, not a hardcoded `engine-cli` scenario. — **done**
- Build whatever minimal gameplay substrate the first playable milestone actually demands — input handling is the near-certain first piece, since nothing in the engine reads player input yet. — **done**: input handling (and, as a prerequisite it surfaced, a live/windowed run loop) landed; see implementation notes below.
- Prototype one of ADR-0009's two asset-generation patterns against a real asset this game needs, and record what was learned. — **done**: the headless-Blender-scripting pattern was prototyped against the "pillar" obstacle, then extended to a small deterministic prop kit (a second color/bevel variant of the same crate) — see the follow-up notes below.

**Definition of done**: something a person can run and interact with, built without adding engine capability that isn't demanded by this game. Met for the first milestone: `cargo run -p sandbox` (or `xvfb-run cargo run -p sandbox` in a headless environment) opens a window and lets a player roll a ball around an arena with WASD, using live physics and rendering.

**Implementation notes**: the first playable milestone — a physics playground (WASD rolls a ball around a walled arena with an obstacle pillar, `games/sandbox/scenes/playground.toml`) — needed far more than a new scene file: the engine had no windowed presentation, wall-clock pacing, or input capture at all (Phase 2's "windowed mode via winit" text was aspirational, never built; confirmed zero `winit` references anywhere pre-Phase-8). See [ADR-0010](docs/decisions/0010-live-input-and-windowed-run-loop.md) for the full design: a narrow `engine_core::{Input, KeyCode}` reusing the `Resources` extension point from ADR-0008 with zero new mechanism; `engine-render`'s `gpu.rs` split into a persistent `GraphicsCore`/`RenderContext` (reused across frames, not rebuilt every call) plus a new `WindowRenderer` for `wgpu::Surface` presentation; a new `engine_cli::live::play` fixed-timestep-accumulator loop and `engine play` CLI command (CLI-only, no MCP tool, same posture as `--watch`); and `PhysicsState::apply_force`, the one new `engine-physics` API `games/sandbox`'s own `PlayerControl` component/system needed. `games/sandbox` itself gained both a `[lib]` and `[[bin]]` target (matching `engine-cli`'s own shape) and is the first crate outside `crates/engine-*` in the workspace — its `main.rs` builds an *extended* registry (`engine_cli::registry::components()` plus its own `PlayerControl` registration) with zero new registry mechanism, the concrete proof Phase 8 set out to get. A real, reproducible SIGSEGV was found and root-caused (via `gdb`) during this work — a Vulkan-loader concurrency bug in debug-utils object-labeling, unrelated to Weft's own logic but triggered much more often by the refactor's timing; fixed by disabling debug/validation `InstanceFlags` (ADR-0010 has the full account, kept deliberately honest about which of several attempted fixes was the real one). `cargo test --workspace` (90 tests) + clippy + fmt all pass; a `#[ignore]`d subprocess test (`games/sandbox/tests/play.rs`) drives the real binary through `--max-ticks` to a clean exit and was verified passing under `xvfb-run` (this sandbox environment needed `xvfb` plus a handful of X11 client libraries installed — a new environment prerequisite alongside ADR-0004's Mesa one).

**Re-evaluation checkpoint, done**: the `Resources` extension point (ADR-0008) and the `ComponentRegistry`/`SystemRegistry` caller-supplied-registry design (ADR-0003) both held on their first real test from *outside* `engine-cli` entirely, needing zero new mechanism — strong continued evidence for both. The one genuine gap these phases hadn't anticipated was rendering itself never having been built for reuse across frames (every phase through 2 only ever needed one-shot offscreen PNG export) — not a design mistake, just the first time a live loop actually needed it, exactly the "let real usage demand the shape" principle this phase exists to test.

**Bug found after first real play-testing, fixed same day**: the user tried the sandbox on their own machine and reported tapping a movement key sent the ball shooting all the way to the nearest wall instead of stopping when the key was released. Root cause: rapier's `add_force` does **not** clear itself after stepping (its own doc comment says a force "keeps being applied at every physics step until you change it or clear it") — `physics_step` never called `reset_forces`, so a single tick's `apply_force` call kept pushing the body every subsequent tick forever, not just the tick it was called on. `PhysicsState::apply_force`'s original doc comment asserted the opposite as fact, untested — a real design-review miss, not just a missing test. Fixed by calling `reset_forces` on every body at the end of each `physics_step`. Separately (and this alone would not have fixed the reported symptom, only bounded it), `RigidBody` gained scene-authorable `linear_damping`/`angular_damping` fields (defaulting to `0.0`, rapier's own default, so existing scenes/tests are unaffected) — without damping, a rolling sphere loses very little speed to plain contact friction and would still coast indefinitely once moving even with forces correctly single-tick. `games/sandbox/scenes/playground.toml`'s player ball now sets both to `4.0`. Two new regression tests in `engine-physics` (`apply_force_only_affects_the_tick_it_was_called_on`, `linear_damping_decelerates_a_body_once_no_force_is_applied`) and a rewritten `games/sandbox/tests/player_control.rs::releasing_the_key_lets_the_ball_decelerate` (measuring instantaneous per-tick speed at the right moments, not cumulative distance, which is misleading while the ball is still accelerating from rest) cover this permanently. `cargo test --workspace` (93 tests) + clippy + fmt all pass.

**Follow-up (2026-09-01, same day): camera-follow.** A second game-specific component/system, `games/sandbox/src/camera_follow.rs`'s `CameraFollow`/`camera_follow_system`, follows the same pattern `player_control` established (external-consumer-defined, registered onto engine-cli's base registry — no new engine mechanism). It queries for whichever entity has `PlayerControl` (stable-sorted by entity id, per ADR-0002, in case that's ever not unique) and positions the camera at a fixed offset from it each tick, with a separate `look_offset` for `Camera.target`; must run after `physics` in scene order so it reads the ball's post-physics position, not last tick's. `cargo test --workspace` (96 tests) + clippy + fmt all pass.

**Follow-up (2026-09-01, same day): first real asset import, prototyping ADR-0009.** The "pillar" obstacle (previously a plain hardcoded cube) is now `games/sandbox`'s first imported asset — a beveled crate mesh, procedurally generated by a new `tools/asset-gen/generate_crate.py` and exported as GLB via headless Blender (`blender --background --python ... -- --output ...`), then run through the existing, unmodified `engine import` and wired into `playground.toml` as `mesh = { asset = "<hash>" }`. This is ADR-0009's "revisit when" trigger firing exactly as anticipated, and it validates the decision: `engine-assets`/`engine-render` needed **zero changes** — the same source-agnostic import path Phase 3 built handles agent-generated content identically to hand-authored content, confirming ADR-0009's core premise. Verified visually correct (not just "imported without erroring") via a throwaway example rendering the live scene with the sandbox's own extended registry to a PNG — the crate appears at the right size/color/position next to the ball, walls, and camera-follow view (deleted after use; not a permanent tool, since nothing yet demands one).

Real friction hit, worth recording since it'll recur for the next asset too: this environment's Debian-packaged Blender needed `liblapack3`/`libblas3` (missing shared libs at Blender's own startup) and `python3-numpy` (Blender's bundled glTF exporter add-on imports `numpy`, and this packaging uses the *system* Python rather than bundling its own) installed before `--background` mode worked at all — neither was mentioned by ADR-0009's original research, which reasonably assumed a real developer's Blender install (from blender.org, which bundles its own complete Python+numpy) rather than a Linux distro package. Not a design problem, just a new environment-setup note in the same spirit as ADR-0004's Mesa driver and ADR-0010's Xvfb/X11 ones.

**Follow-up (2026-09-01, later still): parameterized the prototype into a small prop kit.** `tools/asset-gen/generate_crate.py` gained `--color`/`--bevel-width` arguments (defaulting to the original crate's exact values, verified byte-for-byte — re-running with no arguments still content-addresses to the same `9fa2a8c2...` hash as before). A second, deliberately distinct invocation (`--color 0.5 0.5 0.55 --bevel-width 0.08`, matching the walls' steel-grey) produced a genuinely different file, content-addressed to a new hash (`cfd8be50...`), imported into `games/sandbox/assets` and wired into `playground.toml` as a second obstacle, `pillar_steel`. This is the "prop kit" case ADR-0009 names as headless scripting's natural fit, now actually exercised rather than just described: the same script, same pattern, producing more than one deterministic variant. Verified visually via the same throwaway extended-registry render used for the first crate (built, run under `xvfb-run`, confirmed both crates render correctly side by side, deleted immediately after — still not a permanent tool). `cargo test --workspace` (96 tests) + clippy + fmt all pass, unaffected by this change (no new engine mechanism, same as the first prototype).

---

## Not yet scheduled (deliberately)

These are real future needs but don't have a phase number yet because scoping them now would be speculative ahead of the constraints Phases 0–8 will surface: audio, networking/multiplayer, a packaging/export pipeline for shipping builds, animation blending/state machines beyond the basics, and any optional GUI layer (which, per [ADR-0001](docs/decisions/0001-rust-3d-from-scratch.md) and the engine's core thesis, must be built strictly on top of the same CLI/API surface, never as a capability's only entry point).

---

## Practice: keep re-evaluating

The user's explicit instruction for this project is to *not* treat any of this as fixed — constraints that only become visible during real implementation should actively change the plan. Concretely:

- **At the end of every phase above**, before starting the next one, explicitly revisit: does the ECS choice still feel right? Is the scene text format's schema holding up, or has it needed awkward workarounds? Is `wgpu` giving the right level of control? Has anything from `research/` turned out to be wrong once tested against real code?
- **When a decision changes**, write a new ADR (or mark an existing one "superseded by") in `docs/decisions/` rather than silently drifting — the point isn't ceremony, it's leaving a trail so a future session (agent or human) understands *why* the architecture looks the way it does, not just what it currently looks like.
- **This file should be edited directly** as phases complete, scope shifts, or new phases get added — it is not a historical record, `docs/decisions/` is. Keep `ROADMAP.md` describing the current plan, and let the ADRs carry the history of how it got there.
