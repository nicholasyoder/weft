# Design Principles for Agent-Native Game Engines

*Research and architectural recommendations for a game engine built and operated primarily by AI coding agents.*

## Framing

Every existing engine was designed for a human sitting at a monitor, dragging nodes in a viewport, eyeballing a running game, and clicking "merge" in a GUI diff tool when Git complains. An agent has none of those affordances by default: it has a filesystem, a shell, and (at best) a way to run commands and read their output. If the engine's canonical representation of truth — scene data, asset metadata, build state, runtime state — is not natively legible to a process that reads and writes text and executes programs, the agent is forced to build an entire simulated human (screen capture, mouse/keyboard emulation, OCR, vision models) just to do what a GUI-based human does directly. That's an enormous, brittle detour. The core thesis of this document is:

> **An agent-native engine should make every operation a human would perform with eyes and a mouse instead performable with a text editor, a compiler/interpreter, and a CLI — and should make the *feedback* from each operation come back as structured, parseable, low-noise text (or as an image only when nothing else will do).**

Everything below is a specialization of that thesis.

---

## 1. Scene and asset data formats: text, diffable, git-mergeable

**Principle:** All authorable game data — scenes, prefabs/blueprints, entity definitions, dialogue, level layouts, tuning tables — must be stored as plain text in a documented, line-oriented, human-and-machine-readable format (TOML/JSON/a small custom text DSL). Binary formats are reserved strictly for bulk sample data (mesh vertex buffers, texture pixels, audio waveforms) that no one, human or agent, edits by hand.

**Why it matters for agents specifically:** An agent's primary tools are `read_file`, `edit_file` (string/diff based), and `grep`. These tools are 100% effective on text and 0% effective on binary. A human editing a binary `.uasset` or Unity `.unity` scene at least has a GUI that deserializes the binary into widgets; an agent has no such intermediary unless one is built and kept running. Concretely:

- Unreal's `.uasset` format is not just binary but stores compiled Blueprint bytecode (Kismet VM instructions as jump offsets and flat instruction streams) that has no faithful text projection — decompilers like KismetKompiler only partially reconstruct UE4 Blueprints and don't support UE5 at all. An agent with filesystem access literally cannot read the logic in a Blueprint asset. Community teams have had to reimplement Python-console bridges *six separate times* because there's no native text substrate to build on top of — the binary format isn't merely inconvenient, it's a hard wall that pushes all AI integration into a live-editor RPC proxy (see MCP discussion below) rather than direct file I/O.
- Unity's `.unity`/`.prefab` files are nominally YAML, but in practice are an "incomprehensible soup" of GUIDs, `fileID` cross-references, and serialized-property blobs that are technically diffable but not *meaningfully* diffable or editable by hand — you can see that lines changed, not what changed semantically. Unity ships `UnityYAMLMerge` specifically because naive line-based git merges corrupt these files; that tool exists to compensate for a format problem, not because prefabs are actually simple.
- Godot's `.tscn` format is a genuinely human/agent-legible TOML-like text format: node names, transforms, and script attachments are readable key-value structures, and multiple sources note it's dramatically easier to reason about and merge than Unity's YAML. It's not perfect — any new node changes the file's `load_steps` header and shifts resource sub-indices, which produces spurious diff noise and conflicts even when semantically nothing overlapping changed — but the failure mode is "annoying line near the top of the file," not "opaque byte blob."
- glTF's split of a plain, compact JSON scene graph (nodes, meshes-as-references, materials, animation channels) from a separate `.bin` payload (vertex/index/keyframe data) is the cleanest existing precedent for the right boundary: put anything an agent or human would ever want to *read or edit the meaning of* in JSON; put anything that's just numeric payload that only a GPU or importer touches in binary. This is exactly the split an agent-native engine should copy for its own scenes and assets.

**Trade-offs to accept knowingly:**
- Text formats are larger on disk and slower to parse than binary for scene *graphs*; this is irrelevant for engine authoring workflows and only matters at final ship/runtime, where you should compile text sources down to a binary runtime format anyway (this is exactly what glTF, and Godot's `.tscn`→binary/`res://` import step, already do).
- A hand-designed text scene format that changes index/counters on every save (like `.tscn`'s `load_steps`) will generate noisy diffs and conflicts; design the format so derived/counted fields are either omitted, recomputed on load, or written in a canonical stable order so two independent edits to different entities never touch the same line.
- Avoid GUID/fileID-style indirection wherever a stable human-readable path or name will do — GUIDs are exactly what makes Unity YAML unreadable, since every reference is an opaque hash instead of a name.

**Recommendation:** Canonical scene/entity/asset-definition format = a single well-specified text DSL (TOML or a restricted JSON superset) with stable, name-based (not GUID-based) references, one entity/record per contiguous text block, and no engine-recomputed counters stored inline. Bulk binary payloads (meshes, textures, audio) live in separate content-addressed binary files referenced by path/hash from the text file, mirroring glTF. A build step compiles text sources into a packed runtime format for shipping; the text format is never touched by anything except editors (human or agent) and version control.

---

## 2. ECS architecture over deep OOP inheritance

**Principle:** Structure gameplay as data-oriented ECS (components = plain data structs, systems = free functions/queries over component sets), not class hierarchies with virtual dispatch and multi-level inheritance.

**Why it matters for agents specifically:** The central failure mode of LLM-driven code editing is that changes in a deeply coupled codebase have non-local, hard-to-predict effects — an agent editing `Enemy : Character : Actor : GameObject` has to load and reason about the entire chain (and every override anywhere in it) to safely add behavior, and a single subtly wrong override can silently break unrelated subclasses. ECS sidesteps this structurally:

- Adding a new capability is *additive*: define a new component (a small, self-contained data struct) and a new system (a small, self-contained function that queries for that component). Nothing about existing entities, components, or systems has to be read, understood, or touched. This maps directly onto how an agent works best — small, independently verifiable diffs — rather than the "must understand the whole hierarchy before touching it" mode that inheritance forces.
- ECS "favors composition over inheritance": entities are built by attaching independent components, and behavior changes by adding/removing components rather than editing a class or its ancestors. This is precisely the property that makes an ECS codebase decomposable into units small enough to fit in an agent's working context and safe enough that two agents (or two sequential agent turns) can touch different systems/components without conflicting.
- The data-oriented layout (components stored in contiguous arrays, e.g. Bevy ECS, EnTT, Flecs) is also just good engineering — cache-friendly iteration and easy parallelism — but the decisive reason for an *agent-authored* engine is the compositional/blast-radius property, not performance.
- Concretely: Bevy ECS deliberately avoids "complex lifetimes, traits, builder patterns, or macros" — components are normal structs, systems are normal functions taking queries as parameters. This lowers the amount of framework-specific magic an agent has to correctly reproduce from memory/training data, versus, say, Unity's `MonoBehaviour` lifecycle methods or Unreal's macro-heavy UCLASS/UPROPERTY reflection system, both of which require the agent to get invisible boilerplate exactly right or fail silently (a missing macro doesn't error, it just doesn't work).

**Recommendation:** Adopt (or build in the spirit of) an EnTT/Flecs/Bevy-style ECS as the gameplay substrate: components are plain serializable data, systems are pure(ish) functions over queries, no runtime inheritance between component types, no virtual dispatch in the hot path. Keep the ECS crate/library boring and unsurprising rather than clever — an agent that has to intuit macro-generated behavior will get it wrong more often than one composing explicit function calls.

---

## 3. Headless operation and CLI-first tooling

**Principle:** Every engine capability — importing assets, compiling scripts, running the game, running a specific level, running the test suite, capturing profiling data, exporting a build — must be invokable from a shell command with no GUI, no window server, and no human present, and must produce a machine-checkable exit code and structured output.

**Why it matters for agents specifically:** An agent's edit-run-observe loop *is* a shell loop. If "run the game" means "open an editor window, click Play, look at it," the agent cannot do that without an entire separate GUI-automation subsystem (and even then, GUI automation is slow, flaky, and mostly answers "did a window appear," not "did the game do the right thing"). GUI-dependent engines make the *cheapest, most frequent* agent action (iterate and check) also the *most expensive and least reliable* one, which inverts the cost structure an agent loop needs.

- Godot already gets most of the way there: `godot --headless` runs with no window, no `Xvfb` required, and community test runners (GUT, gdUnit4) drive full unit and scene-tree-simulation tests from a CLI, returning proper exit codes (`-gexit_on_complete` for GUT) that plug straight into CI. This is close to the right model.
- Bevy ships with **no built-in editor at all** — the entire engine is a Rust library invoked from code, run with `cargo run`, tested with `cargo test`. This is the most agent-friendly posture achievable: there is no GUI to route around because the primary interface was never a GUI in the first place. The trade-off is that Bevy currently pushes scene-authoring conveniences (that a human would get from a GUI) back onto text/code, which is fine for an agent but would be painful for a human-only team — an agent-native engine can happily make this trade since the tool is exactly the thing that removes the pain.
- The practical bar: `engine build`, `engine run <scene>`, `engine test [pattern]`, `engine import`, `engine profile <scene> --frames N`, `engine export <target>` should all exist as first-class subcommands with zero required interactive input, sensible non-zero exit codes on failure, and `--headless` as the *default*, not an opt-in flag.

**Recommendation:** Design the engine as a library + CLI first; treat any future GUI/editor as an optional, strictly additive layer built *on top of* the same CLI/API surface, never as the primary or only interface to a capability. If a feature can only be done through a GUI, it is a bug in the architecture, not a missing convenience.

---

## 4. The agent-perception problem

**Principle:** Prefer structured, textual state inspection over pixels wherever the question can be answered without pixels; reserve screenshot + vision-model (VLM) feedback for genuinely visual concerns (layout, color, readability, "does this look right") and make that path a deliberate, explicit tool call, not the default way an agent checks its work.

**Why it matters for agents specifically:** A human glances at a running game and instantly extracts "the player is at the wrong position," "the health bar is invisible," "that enemy is T-posing." An agent has no eyes by default, and even with a VLM in the loop, image-based feedback is expensive (extra model call, latency, tokens), imprecise (a VLM can miss a one-pixel-off UI element, or hallucinate what it sees), and hard to turn into a reliable pass/fail assertion. The cheap, robust alternative is to make the running (or a headless-simulated) game *dump its own state as data*:

- **Programmatic world-state dumps.** The engine should expose a command like `engine inspect --scene X --frame N --format json` that serializes the entire ECS world (every entity's components, transforms, current animation state, active timers) to JSON. An agent can then assert exact facts ("entity `player` has `Health.current == 80`," "exactly 3 entities have the `OnFire` component") with normal string/JSON diffing — no vision model, no ambiguity, deterministic pass/fail. This should be the *first* tool an agent reaches for to verify gameplay logic.
- **Deterministic replay/recording** (see also §5): record the input stream and RNG seed for a session, and provide `engine replay <recording> --dump-state-every N` so any observed bug is exactly reproducible and inspectable without pixels at all.
- **Visual regression as a second-tier, explicitly visual check.** For things that are inherently about pixels — did the shader compile to something that renders correctly, is the UI actually laid out sanely, is there a glitch/clipping issue — headless rendering to an offscreen buffer plus image diffing (à la Percy/Playwright screenshot diffing, or the game-specific analogue "VideoGameQA-Bench" work on VLM-based QA) is the right tool. Recent research on VLM-based visual regression testing notes that naive pixel-diffing over-fires on cosmetically-irrelevant variation (lighting, weather, character customization) and that a VLM prompted with before/after images can describe *what* changed in plain language ("the button in the top-right is now taller and misaligned with the search bar") — which is far more actionable for an agent than a raw diff heat-map. This VLM-narration pattern is the right design when pixels are unavoidable: don't hand the agent an image and hope it "looks"; hand it a textual description of the diff generated by a vision step, and let the agent's normal text reasoning take it from there.
- Ordering of preference for the agent, cheapest/most reliable first: (1) structured JSON world-state assertion, (2) deterministic replay + state dump at a specific frame, (3) headless render + programmatic pixel/structural diff against a reference image, (4) VLM-narrated screenshot diff, only when the question is inherently about visual appearance and can't be phrased as a data assertion.

**Recommendation:** Build `engine inspect` (world state → JSON) and `engine replay` (deterministic re-run with state dumps) as first-class, day-one CLI commands — these are cheap to build and eliminate the large majority of situations where an agent would otherwise need a screenshot. Add headless-render-to-PNG plus an image-diff tool as a secondary capability for visual/UI work, and route its output through a short VLM-generated natural-language description rather than expecting the agent to reason over raw pixels or raw diff images directly.

---

## 5. Testing and determinism

**Principle:** The simulation must be deterministic under a fixed timestep and seeded RNG, so that "same inputs → same trace of world states, forever" holds. This is the load-bearing precondition for every kind of automated test an agent can write.

**Why it matters for agents specifically:** An agent writes regression tests the same way it writes code — by generalizing from an example. If running a scene twice produces two different outcomes (variable timestep, unseeded `rand()`, HashMap iteration order, floating-point nondeterminism across threads), the agent cannot tell the difference between "my fix broke this" and "this game is just nondeterministic," and every snapshot/assertion-based test becomes flaky. Flaky tests are worse than no tests for an agent loop, because the agent will "fix" a nondeterminism symptom by patching the wrong thing, or will learn to ignore failing tests altogether.

- **Fixed timestep**: physics/gameplay update must run at a fixed `dt` (Unity's own physics defaults to a fixed 0.02s step for exactly this reason; simulation frameworks used for autonomous-vehicle verification, e.g. CARLA, use a fixed 0.05s step specifically because reproducibility requires it). Rendering can run at a variable/interpolated rate on top of a fixed simulation step, but the simulation step itself must not depend on wall-clock jitter.
- **Seeded RNG threaded explicitly**: never call a global/ambient RNG; pass a seeded RNG instance through the call graph so the entire simulation's behavior is pinned to one seed. This also means the engine's own systems (e.g., iteration order over an ECS query) must be defined and stable, not "whatever the hash map happened to do today."
- **Lockstep/input-recording replay**: record the input stream (and seed) rather than the resulting state; replaying inputs against a deterministic sim reproduces the exact run. This is the standard architecture for RTS replays and is directly reusable as the engine's regression-test format: a test *is* a recorded input sequence plus an expected final (or per-frame) world-state snapshot.
- **Snapshot testing discipline**: treat every bug found via a specific seed/input-recording as a permanent regression test that stays in the suite forever, exactly as recommended in deterministic-simulation-testing practice — once an agent fixes a bug, the recording that exposed it becomes a cheap, permanent guard against the same class of regression, checked automatically on every future change.

**Recommendation:** Make determinism a hard architectural constraint, not a best-effort goal: fixed simulation timestep, one seeded RNG source threaded explicitly (never ambient), stable iteration order guaranteed by the ECS, and a first-class `engine test` format that is "recorded inputs + seed in, expected world-state snapshot(s) out." Every bug an agent fixes should end with it adding exactly one such recording to the suite.

---

## 6. Scripting/extension layer

**Principle:** Prefer a statically-typed, compiled path with a fast, informative compiler as the *primary* extension mechanism, with hot-reload for iteration speed; use a scripting language only where genuinely fast, low-stakes iteration matters more than compile-time correctness, and treat WASM as the sandboxing/distribution answer, not the daily-authoring answer.

**Why it matters for agents specifically:** This is a real trade-off, not a clean win for one side:

- **Compiled/typed code (Rust, C#, typed native code) gives agents a self-correction signal.** Research on LLM-assisted Rust code repair finds that Rust's diagnostics are "structured enough for LLMs to parse and correct automatically" and that models achieve high automatic-fix rates on common error classes (some categories above 80% resolution) purely from feeding the compiler's own error text back to the model. This is exactly the loop an agent runs naturally: generate code → compiler rejects it → agent reads the diagnostic → agent fixes it — and it requires *no engine-specific tooling at all*, just a good compiler. A dynamically typed scripting language (Lua, GDScript) defers all of that feedback to runtime, where errors are less structured, often only surface much later (e.g., a `nil` field access three calls downstream of the actual mistake), and give the agent a far weaker signal to self-correct from.
- **Scripting languages win on iteration speed and hot-reload.** Lua is the dominant embedded game-scripting language precisely because it's lightweight, fast to embed, and trivially hot-reloadable (edit a `.lua` file, the running game picks it up with no rebuild) — valuable when an agent is doing rapid, low-stakes tuning (numbers, simple triggers, dialogue logic) where a compile cycle would be pure overhead.
- **WASM is a sandboxing and distribution mechanism, not an authoring ergonomics win.** WASM gives memory-safe, sandboxed execution of plugins compiled from any source language, which matters for mod/plugin distribution and untrusted third-party code, but the agent still authors in whatever source language compiles to WASM (typically a typed language, gaining that language's compiler feedback) — WASM itself doesn't change the authoring loop, it changes the deployment/isolation story.
- **Recommended split**: gameplay *systems* (the stuff that defines how the game fundamentally works) should be written in the engine's native compiled language with hot-reload for the native binary itself (e.g., Rust's `hot-lib-reloader`-style patterns, or a scripting shim that only exists for iteration and is deleted/compiled-out for shipping). *Content-level* scripting (quest logic, dialogue trees, per-encounter tuning) is a good fit for an embedded interpreted language, precisely because it's low-stakes, high-iteration, and the failure mode of a mistake there is "wrong dialogue," not "corrupted world state."

**Recommendation:** Native, statically-typed, compiled code (with hot-reload for fast iteration) as the primary substrate for engine and gameplay-system code — the compiler is the single best "self-correction" tool an agent can be given, and it comes for free. Keep an embedded scripting language (Lua-class) available specifically for high-churn, low-risk content authoring, not as the primary way systems get written. Use WASM only where sandboxed third-party plugin distribution is an actual requirement, not as a default authoring format.

---

## 7. Agent tool interface design

**Principle:** Expose the engine to agents through three concrete surfaces — a scriptable CLI with subcommands and machine-parseable output; a small MCP (Model Context Protocol) server exposing the same operations as typed tools for richer agent runtimes; and consistently structured, precise diagnostics from every one of those operations. Hot-reload closes the loop by making the edit-run-observe cycle fast enough to actually iterate in.

**Why it matters for agents specifically:** An agent doesn't need a GUI-replacement, it needs *tools with predictable, well-documented contracts*. The industry has already converged on this exact shape for game engines in 2025–2026: Unity ships an official MCP server built into its AI tooling; independent MCP servers exist for Godot and Unreal (`Godot-MCP`, `unreal-mcp`), and at least one project (`GameDev-MCP-Server`) deliberately factors the protocol bridge as an engine-agnostic server so the same MCP surface can sit in front of multiple engines. The stated purpose across all of these is identical: give the agent "text-shaped tools to query the scene hierarchy, read the console, execute commands" instead of forcing it to operate a GUI by proxy. The lesson for a from-scratch engine is to build this in from day one rather than retrofitting it the way Unity/Unreal/Godot have had to:
- **CLI first, MCP as a thin typed wrapper over the same commands.** Every MCP tool the engine exposes (`spawn_entity`, `run_scene`, `get_world_state`, `run_tests`) should be a direct wrapper around a CLI subcommand or library call that also works standalone from a shell — never MCP-only functionality, so the engine remains scriptable even without an MCP-aware agent runtime, and so there's only one implementation of each operation to keep correct.
- **Diagnostics are a first-class API, not a debugging afterthought.** Every failure — a scene fails to load, a component reference is dangling, a system panics, an asset fails to import — should report machine-parseable structured output (a stable error code/kind, the offending file and line/entity, and a plain-language description), on the model of a good compiler diagnostic rather than a stack trace dump or a silent no-op. The Blueprint-in-Unreal failure mode noted earlier — "a single small syntax error is often enough for the interpreter to fail to parse the request, with no error surfaced" — is exactly what to avoid: a request that silently does nothing gives an agent zero signal to correct from, which is strictly worse than an error.
- **Hot-reload matters for tool ergonomics, not just human convenience.** If every edit round-trips through a full engine restart, the agent's loop-latency (and therefore cost, since every iteration also consumes tokens/time) balloons; native hot-reload for code and instantaneous reload for data/scene text files should be treated as a core performance requirement of the *agent* workflow, not a nice-to-have for humans.

**Recommendation:** Build the CLI and its structured JSON output mode first; layer a thin MCP server on top exposing the same operations as typed tools, matching the pattern Unity/Godot/Unreal have each converged on independently. Invest specifically in diagnostic quality (structured, precise, always-emitted-on-failure) as a top-tier engineering priority, on par with runtime performance — it is the single highest-leverage lever for agent self-correction. Support hot-reload for both code and data as a baseline requirement.

---

## 8. Version-control friendliness and parallel-agent workflows

**Principle:** Structure the project so that (a) nothing binary and mergeable-in-principle is ever the sole representation of important state, and (b) the natural unit of agent work (one system, one entity type, one scene) maps to one or a small number of files, so that multiple agents (or multiple sequential turns) touching different game features rarely touch the same file.

**Why it matters for agents specifically:** Multi-agent and multi-session workflows are increasingly normal (parallel subagents each owning a feature, or a long agent session working across many commits); the value of that parallelism collapses if every agent's change lands in the same giant scene file or a monolithic binary blob that nobody can merge. The evidence from existing engines is consistent:
- Binary blobs (`.uasset`, compiled `.unity3d`, etc.) produce "two completely different blobs" on concurrent edits with no automatic merge and no diff — the only real mitigations are file locking (Perforce-style checkout) or specialized binary-aware VCS, both of which serialize work and defeat parallel agents by construction.
- Even Godot's text `.tscn` format, which is fundamentally mergeable, still generates avoidable conflicts from incidental churn (the recalculated `load_steps` header, shifting resource sub-indices) — a lesson to design out explicitly in a new format rather than repeat.
- The fix is architectural, not tooling: prefer **one file per entity-type/prefab/system** over "one giant level file with everything in it," use stable content-addressed or name-based references instead of GUIDs/incrementing indices so unrelated edits don't perturb shared counters, and keep systems (code) and their data (components/config) in files scoped tightly enough that two agents working on two different game features are extremely unlikely to touch the same file, let alone the same line.

**Recommendation:** Default project layout = one text file per entity template/prefab and per system, not monolithic scene or class files; no engine-recomputed shared counters stored inline in text files; stable name/hash-based references instead of GUIDs; large binary content kept in content-addressed files that are never hand-edited and therefore never merge-conflicted in the meaningful sense (a changed hash just means "replaced," which is an unambiguous merge outcome). This turns "many agents working in parallel" from a hazard into the expected, well-supported mode of development.

---

## Summary table

| Area | Reject | Adopt |
|---|---|---|
| Scene/asset format | Binary (`.uasset`), opaque-YAML (Unity) | Text DSL + separate binary payload (glTF/`.tscn`-style) |
| Architecture | Deep OOP inheritance | Data-oriented ECS (Bevy/EnTT/Flecs-style) |
| Operation | GUI-required workflows | CLI-first, `--headless` default, library+CLI, no built-in editor requirement |
| Feedback | Only pixels/screenshots | JSON world-state dumps + deterministic replay first; VLM screenshots second |
| Testing | Wall-clock/unseeded nondeterminism | Fixed timestep, explicit seeded RNG, input-recording snapshot tests |
| Extension | Scripting-only or WASM-by-default | Compiled+typed primary (compiler-driven self-correction) + hot-reload; scripting for content only |
| Interface | Ad hoc / GUI automation | CLI with structured output + thin MCP wrapper + first-class diagnostics |
| VCS layout | Monolithic scene/binary blobs | One file per entity/system, name-based refs, content-addressed binaries |

## Sources

- [MCP servers and game development: What they are and why they matter — Unity](https://unity.com/blog/mcp-servers-game-development)
- [Godot-MCP — Model Context Protocol integration for Godot](https://github.com/IvanMurzak/Godot-MCP)
- [GameDev-MCP-Server: engine-agnostic MCP server for Unity/Godot/Unreal](https://github.com/IvanMurzak/GameDev-MCP-Server)
- [The .uasset Problem: Why Unreal's Binary Format Is Holding Back AI Workflows — Sackbird Studios](https://www.sackbirdstudios.com/news/uasset-binary-problem)
- [Version Control for .uasset Files — Diversion](https://www.diversion.dev/knowledge-center-articles/uasset-version-control)
- [Pain of Unreal Engine: binary assets — Epic Developer Community Forums](https://forums.unrealengine.com/t/pain-of-unreal-engine-binary-assets/24394)
- [.TSCN merging tooling · Issue #1281 · godotengine/godot-proposals](https://github.com/godotengine/godot-proposals/issues/1281)
- [Resolving conflicts in Unity scenes with git and UnityYAMLMerge](https://kihontekina.dev/posts/git_conflicts/)
- [Godot and Git (part 7): Tips for merging scenes and team collaboration — Snopek Games](https://www.snopekgames.com/tutorial/2020/godot-and-git-part-7-tips-merging-scenes-and-team-collaboration/)
- [Version control for your Godot game projects — Diversion](https://www.diversion.dev/blog/version-control-for-your-godot-game-projects)
- [glTF 2.0 Specification — Khronos Registry](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html)
- [GLTF File Format: What It Is and How to Use It — VNTANA](https://www.vntana.com/blog/gltf-file-format/)
- [ECS quick-start — Bevy](https://bevy.org/learn/quick-start/getting-started/ecs/)
- [bevy_ecs — docs.rs](https://docs.rs/bevy_ecs/latest/bevy_ecs/)
- [ECS 1: Inheritance vs Composition and ECS Background — LeatherBee Games](https://leatherbee.org/index.php/2019/09/12/ecs-1-inheritance-vs-composition-and-ecs-background/)
- [ECS vs OOP in Large-Scale Game Development — Daydreamsoft](https://www.daydreamsoft.com/blog/ecs-vs-oop-in-large-scale-games-choosing-the-right-architecture-for-performance-and-scalability)
- [Run automated tests for your Godot game on CI — David Saltares](https://saltares.com/run-automated-tests-for-your-godot-game-on-ci/)
- [CI/CD for Godot Projects: GUT + GitHub Actions — helpmetest.com](https://helpmetest.com/blog/godot-ci-cd-testing/)
- [Deterministic Simulation Testing: A Practical Guide for QA Engineers — The Green Report](https://www.thegreenreport.blog/articles/deterministic-simulation-testing-a-practical-guide-for-qa-engineers/deterministic-simulation-testing-a-practical-guide-for-qa-engineers.html)
- [Game Engines and Determinism — Duality.ai](https://www.duality.ai/blog/game-engines-determinism)
- [On Determinism of Game Engines used for Simulation-based Autonomous Vehicle Verification (arXiv)](https://arxiv.org/pdf/2104.06262)
- [VideoGameQA-Bench: Evaluating Vision-Language Models for Video Game Quality Assurance](https://arxiv.org/html/2505.15952v2)
- [Human-AI Collaborative Game Testing with Vision Language Models](https://arxiv.org/html/2501.11782)
- [Visual regression as a feedback loop for agents — stevekinney.net](https://github.com/stevekinney/stevekinney.net/blob/main/courses/self-testing-ai-agents/visual-regression-as-a-feedback-loop.md)
- [Rust and LLMs: The Compiler Does What Code Review Shouldn't Have To — DEV Community](https://dev.to/arezvov/rust-and-llms-the-compiler-does-what-code-review-shouldnt-have-to-3ia4)
- [RUSTASSISTANT: Using LLMs to Fix Compilation Errors in Rust Code — Microsoft Research](https://www.microsoft.com/en-us/research/wp-content/uploads/2024/08/paper.pdf)
- [Embedding Lua in the Source Engine — Valve Developer Community](https://developer.valvesoftware.com/wiki/Embedding_Lua_in_the_Source_Engine)
- [Reconsider adding a WebAssembly (WASM) API · Issue #12836 — luanti-org/luanti](https://github.com/luanti-org/luanti/issues/12836)
- [AI Coding Tools for Video Game Development: A First-Principles Analysis — Chier Hu (Medium)](https://chierhu.medium.com/ai-coding-tools-for-video-game-development-a-first-principles-analysis-of-what-actually-works-90dfa10edd13)
- [Integrating LLM in Unity: Why I Moved From Embedded Clients to MCP — Medium](https://medium.com/@vladsk.panchenko.97/integrating-llm-in-unity-why-i-moved-from-embedded-clients-to-the-mcp-tools-24bb920f7e85)
