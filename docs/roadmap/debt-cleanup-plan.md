# Architectural-debt & process-gap cleanup plan

**Working checklist, not a spec — same living-document rule as the rest of `docs/roadmap/`.** Addresses the 9 "Architectural debt" items and 4 "Process / testing gaps" items in [`known-issues.md`](known-issues.md) (the 3 Critical items and 5 Real bugs from the same 2026-09-02 review are already fixed). Split into 8 independent phases so risk stays contained — each phase should land as its own reviewable, tested commit-set, with the same gate every phase in this project uses: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check`, all clean before committing.

**Status: Phases 1, 2, 3, 4, 5, 6, and 7 done (2026-09-02).** Mark each phase's heading done (like the Critical items in `known-issues.md`) as it lands, and update the corresponding `known-issues.md` bullet to struck-through/fixed at the same time.

Research behind this plan (three parallel codebase surveys) found the roadmap text itself had drifted in two places, corrected in Phase 1: only 1 of 4 scenarios (not 2) actually lacks a scene-file counterpart, and `engine-anim`'s error-handling "fork" bullet mis-described its current shape — `engine-anim` no longer panics (fixed by the earlier Critical-item work), but it still doesn't own a `thiserror`-based error type the way `engine-scene`/`engine-render`/`engine-audio`/`engine-assets`/`engine-script` each do; it converts a foreign `engine_assets::AssetError` into `SystemError` via a small local helper instead. Not a fixed instance of the same convention — a different, arguably fine design with nothing of its own left to convert.

**Scoping decisions already made** (don't re-litigate without a reason):
- **Include** the `engine-core` "shared vocabulary" crate split (Phase 3) despite its wide blast radius.
- **Defer** GPU per-frame buffer/bind-group pooling — only do the safe pipeline-descriptor dedup (Phase 4). No perf test exists to validate pooling against.
- **Do** the glam version bump (Phase 5) — `nalgebra` (rapier3d's internal math) already has its `glam033` conversion feature active in the dependency graph today, so aligning the workspace's `glam` pin to 0.33 isn't just cosmetic: it lets `engine-physics/src/convert.rs`'s hand-rolled conversions be deleted, not merely justified.
- **Not planned**: the heavier `Resources` "required vs optional" struct split (just adding `remove()` closes the stated gap); a real error type for `engine-physics` (it's genuinely infallible today, nothing to convert).

Recommended order: **1 → 2 → 4 → 6 → 7 → 3 → 5 → 8** — cheap/safe first, the two wide-reaching refactors (3, 5) next-to-last once everything else is stable and isolated from unrelated changes, sandbox coverage (8, the most design-open-ended) last. Phases are independent, so this order isn't load-bearing — just a reasonable default.

---

## Phase 1 — Small, low-risk fixes (one commit-set) — DONE (2026-09-02)

- **`engine-physics/src/system.rs`**: `physics_step`'s pose write-back loop iterates `state.bodies` (a `HashMap<Entity, RigidBodyHandle>`) unsorted, unlike `evict_despawned` 20 lines earlier which sorts by `entity.to_bits()`. Fix: collect+sort a `Vec` of the same entries before the loop, mirroring `evict_despawned`'s existing pattern exactly.
- **`engine-core/src/error.rs`**: delete the empty, unreferenced `SimError` enum and its module registration (confirmed zero references workspace-wide).
- **`engine-core/src/resources.rs`**: add `pub fn remove<T: 'static>(&mut self) -> Option<T>` to `Resources`, closing the "no remove, so a resource lives for the Sim's full lifetime" gap. No other API changes.
- **`engine-cli/src/registry.rs`** + **`games/sandbox/src/lib.rs`**: every `load_x`/`dump_x` pair is already 100% mechanical (`serde_json::from_value::<T>` / `serde_json::to_value(*x or x.clone())`). Replace the ~19 hand-written pairs with two generic functions (`fn load<T: DeserializeOwned>(...)`, `fn dump<T: Serialize + Clone>(...)`) registered as `register("Name", load::<T>, dump::<T>)` — no macro needed.
- **`docs/roadmap/known-issues.md`**: mark the `Resources` (partially — just the "no `remove`" clause), determinism-by-sorting, registry-boilerplate, and `SimError` bullets resolved (this phase's other four changes close each). Correct — without marking fixed — the "scenario/scene split" and "error-handling convention forked" bullets' inaccurate claims: the "2 of 4" scenario count was wrong (it's 1 of 4, `broken-rng`, which can't have a scene-file counterpart by design since it exists to bypass the seeded RNG); the seam itself (`SimSource::build` still hand-re-inserting `AudioSettings::default()` for the hardcoded arm) is unchanged. And `engine-anim` is not "already fixed" into the `.code()` convention — it no longer panics, but has no `thiserror` dependency or `error.rs` of its own, converting a foreign `AssetError` into `SystemError` via a local helper instead; a legitimately different design, not a fixed instance of the same pattern. Only `engine-physics` remains genuinely without an error type, with nothing fallible to convert today.

**Verify**: full gate.

---

## Phase 2 — Bounded/evicting caches — DONE (2026-09-02)

`RenderContext`'s `mesh_cache`/`texture_cache`/`skin_cache`/`font_cache` (`engine-render/src/gpu.rs`) and `engine-audio`'s clip cache (`engine-audio/src/system.rs`'s `AudioCache.clips`) are unbounded `HashMap`s, insert-only, keyed by content hash — a real leak risk in a long-running `engine play`/`--watch` session with hot-reloaded assets.

- Add the `lru` crate (well-maintained, zero-dependency) to `engine-render` and `engine-audio`.
- Replace each of the 5 caches with `lru::LruCache` at a fixed starting capacity (e.g. 256 entries — revisit if a real workload needs tuning). Every existing `contains_key`/`get`/`insert` callsite maps directly onto `LruCache`'s equivalent methods.

**Verify**: full gate — golden-image/WAV tests must stay byte-identical (eviction only matters past 256 distinct assets, which no current fixture approaches).

---

## Phase 3 — Extract `engine-types`, a leaf crate for engine-core's "shared vocabulary" — DONE (2026-09-02)

`engine-core` currently holds `Transform`, `JointPalette`, `AudioSettings`, `SoundEvent`/`SoundEventQueue`, `Input`/`KeyCode`, `AssetsDir` purely so producer/consumer crate pairs (e.g. `engine-anim`→`engine-render` for `JointPalette`) can share a type without a direct dependency edge. No crate currently depends on a sibling crate directly, so this is safe to extract without creating a cycle.

- New crate `crates/engine-types` (leaf: only `glam`, `serde`, `hecs` as needed — no dependency on `engine-core` or anything else in the workspace).
- Move `Transform`, `JointPalette`, `AudioSettings`, `SoundEvent`/`SoundEventQueue`, `Input`/`KeyCode`, `AssetsDir` (each type's whole module, including its own unit tests) from `engine-core/src/` into `engine-types/src/`.
- `engine-core` keeps: the `hecs` re-export, `Resources`, `Scheduler`/`SystemError`, `Sim`, seeded RNG — the genuinely simulation-kernel primitives, not shared data types.
- Every crate currently doing `use engine_core::{Transform, ...}` (engine-physics, engine-anim, engine-audio, engine-scene, engine-script, engine-render, engine-cli, engine-mcp, games/sandbox) adds `engine-types` to its `Cargo.toml` and updates imports. `engine-core` may still re-export these from `engine_types` for convenience if that reduces churn — decide during implementation, either is fine as long as it's consistent. *(Decided: re-export. Traced every actual usage first — every one of those ~26 files importing a moved type also imports a genuine `engine-core` kernel item, e.g. `Sim`/`SystemArgs`/`ComponentDumper`/`Resources`, in the same file, so none of them could drop the `engine-core` dependency anyway; a direct-dependency migration would've been ~9 `Cargo.toml`s and ~20 files of import churn for no real decoupling payoff today. `engine-core`'s `lib.rs` re-exports all 8 items flat instead — behaviorally transparent, since a re-export preserves `TypeId`. Only one file needed a fix: `games/sandbox/src/player_control.rs` imported via the `engine_core::input::KeyCode` submodule path rather than the flat re-export.)*
- Update `ROADMAP.md`'s workspace-layout diagram and `AGENTS.md`'s crate table. *(AGENTS.md has no separate crate table — only inline `engine_core::Input`/`KeyCode` references, unaffected by the re-export — so only ROADMAP.md's diagram needed a change.)*

**Verify**: `cargo build --workspace` first (expect many import-path errors to fix mechanically), then full gate. Do this phase in isolation, not interleaved with others, so a compile break is easy to bisect. *(In practice, the re-export decision meant zero import-path errors from the move itself — the only build failure hit was an unrelated disk-full linker crash from an 87GB stale `target/`, fixed with `cargo clean`.)*

---

## Phase 4 — `engine-render/src/gpu.rs` pipeline-descriptor dedup (pooling deferred) — DONE (2026-09-02)

Three `RenderPipelineDescriptor` blocks (`pipeline_for`, `skinned_pipeline_for`, `ui_pipeline_for`, ~lines 459/509/561) are ~95% identical — differing only in shader module, vertex layout, blend state, depth-write/compare, and cull mode.

- Extract a shared `build_pipeline(...)` helper parameterized by whatever actually varies (shader, vertex layout, blend, depth-write, depth-compare, cull-mode, label — exact signature TBD during implementation) and have all three call sites use it instead of duplicating the descriptor.
- **Not doing** per-frame uniform-buffer/bind-group pooling in `draw()` (scoping decision) — leave `known-issues.md`'s bullet noting this stays deferred until Tier 2/3 actually adds a 4th (shadow) pass.

**Verify**: golden-image tests (`engine-cli/tests/render.rs`) must stay byte-identical/within-tolerance unchanged — pure refactor. Full gate.

---

## Phase 5 — Align the workspace's `glam` pin to 0.33 and simplify `engine-physics/convert.rs` — DONE (2026-09-02)

Two `glam` versions are active today: `0.29.3` (workspace pin) and `0.33.6` (pulled transitively via `rapier3d`→`parry3d`→`glamx`, and `nalgebra`'s `glam033` conversion feature — confirmed active via `cargo tree`). Cargo.lock also carries 3 unused stale glam entries (0.30.10/0.31.1/0.32.1 — confirmed nothing in the workspace resolution depends on them).

- Bump the root `Cargo.toml`'s `glam = { version = "0.29", ... }` to `"0.33"` (resolves to the already-present `0.33.6` — this reduces the active version count to one).
- `cargo build --workspace`, fix whatever glam 0.29→0.33 API changes surface (exact scope unknown until attempted).
- **The actual payoff**: `nalgebra`'s `glam033` feature is already compiled into the dependency graph (enabled transitively by `glamx`), providing `From`/`Into` between `nalgebra` types and `glam 0.33` types. Since `rapier3d::math::Vector`/`Rotation` are `nalgebra` type aliases, once `engine-physics`'s own `glam::Vec3`/`Quat` are the *same* 0.33 types, `crates/engine-physics/src/convert.rs`'s hand-rolled `vec3_to_rapier`/`vec3_from_rapier`/`quat_to_rapier`/`quat_from_rapier` should collapse to `.into()` calls (or be deleted entirely) — confirm this actually works during implementation; keep only whatever manual conversion turns out to still be genuinely needed.
- `cargo update -p glam` (or equivalent) to prune the now-fully-unused 0.30/0.31/0.32 lockfile entries.
- Spot-check golden image/WAV fixtures aren't affected (expect zero change, but verify rather than assume).

**Verify**: full gate, plus explicit attention to `engine-cli/tests/render.rs` and `engine-cli/tests/physics.rs`/`determinism.rs` since this phase touches the math layer under physics.

*(In practice: `rapier3d::math::Vector`/`Rotation` turned out not to be `nalgebra` types at all — `glamx` (rapier3d's own math backend, confirmed by reading its source in the local registry cache) defines them as direct re-exports of `glam::Vec3`/`glam::Quat`. So once the workspace pin matched, `convert.rs`'s 4 functions were straight identity — the file was deleted outright rather than rewritten to `.into()` calls, and its 6 call sites in `engine-physics/src/system.rs` now pass/read `glam::Vec3`/`Quat` directly. Zero API-surface breakage from the version bump itself (confirmed against glam's own CHANGELOG before touching code — nothing used by this codebase changed between 0.29 and 0.33), aside from two functions moving behind a deprecation notice: `Mat4::perspective_rh`/`Mat4::look_at_rh` → `glam::camera::rh::proj::directx::perspective`/`glam::camera::rh::view::look_at_mat4` (same NDC convention, updated in `engine-render/src/gpu.rs`, golden-image tests stayed byte-identical). The stale 0.30/0.31/0.32 lockfile entries stayed — they're optional-feature pulls on `nalgebra`, independent of the workspace's own glam pin — not chased further since the real payoff didn't depend on it.)*

---

## Phase 6 — CI (GitHub Actions) — DONE (2026-09-02)

No CI exists anywhere (repo is on GitHub). AGENTS.md's hard gate is enforced by agent discipline alone today.

- New `.github/workflows/ci.yml`: single Ubuntu job installing `mesa-vulkan-drivers` (golden-image render tests need Mesa's `lavapipe` software Vulkan ICD) and `libasound2-dev` (needed for `cpal`/ALSA to compile, per ADR-0016) via `apt-get`, then running the three gate commands. Cache `~/.cargo` and `target/` via `actions/cache` keyed on `Cargo.lock`.
- Deliberately **not** in CI scope: the `#[ignore]`d windowed-`play` subprocess test (needs Xvfb — stays local/manual) and anything invoking Blender.
- **`tools/asset-gen/generate_crate.py`** test coverage: its `parse_args()` is pure `argparse`, zero `bpy` dependency. Add `tools/asset-gen/test_generate_crate.py` (pytest) covering defaults and `--color`/`--bevel-width` parsing, plus a `pytest` step in the same CI job. `main()`'s actual Blender generation logic stays uncovered by design (consistent with ADR-0009's workspace-boundary call).

**Verify**: push the workflow and confirm a real run goes green on GitHub — this phase's verification is inherently remote.

---

## Phase 7 — MCP error-path test coverage — DONE (2026-09-02)

`engine-mcp/tests/tools.rs` exercises exactly 2 error codes (`SCENE_READ_ERROR`, `SCENARIO_NOT_FOUND`) across all 7 tools, reusing an existing, reusable harness: `connect()` (spawns the real `engine-mcp` subprocess over stdio), an `args(json!({...}))` helper, and the assertion shape `assert_eq!(result.structured_content.unwrap()["error"]["code"], "...")`.

- Add tests (same file, same harness pattern) for: `SIM_SOURCE_CONFLICT` (both `scenario` and `scene` given to `test`/`inspect`), `INVALID_TICKS` (`ticks: 0` — cover at least one representative tool, all 5 if cheap), and one error path each for `weft_replay`, `weft_render`, `weft_mix`, `weft_import` (reusing fixtures the CLI-level tests already established for the same errors).
- Add one new probe for what `rmcp`'s own JSON-schema deserialization does with a malformed/missing-required-field call *before* a tool body runs — bypasses `CliError` today, and AGENTS.md calls an unobserved case "a bug worth filing." Observe and assert on actual current behavior; only chase a code fix if it's actually broken.

**Verify**: `cargo test -p engine-mcp`, then full gate.

---

## Phase 8 — Sandbox animation + looping-audio coverage

`games/sandbox/scenes/playground.toml` has no `Animator`/skinned mesh and no `[audio]` table — only one-shot SFX is exercised. No cross-subsystem test surface exists anywhere for animation+physics, animation+rendering, or looping-audio+despawn in a real game scene.

- Import the existing hand-built two-joint fixture (`crates/engine-assets/tests/fixtures/skinned.gltf`, already proven deterministic, used by `engine-cli/tests/animation.rs`) into `games/sandbox`'s real asset store via `engine import`.
- Add one entity to `playground.toml` with `MeshRef.skin` + `Animator` wired to the imported skin/skeleton/clip (playing, looping).
- Add a top-level `[audio]` table (schema already exists — `engine_scene::format::AudioMeta` — just unused today) and a looping `AudioSource` on an ambient entity.
- New `games/sandbox/tests/*.rs` test(s): assert the animated entity's joint palette actually changes over ticks, and cover the sandbox's own looping-audio behavior (reusing the `mix_despawn.toml`-style pattern from the 5-bugs pass if a despawn interaction fits naturally, otherwise just asserting the loop persists across ticks — decide during implementation based on `playground.toml`'s actual entities).
- **Explicitly out of scope**: authoring a *properly rigged/animated* asset through `tools/asset-gen`'s headless-Blender pipeline — zero skinning/animation export support exists there today; building it is a separate, larger effort ADR-0015 already flags as a deliberate future step.

**Verify**: `cargo test -p sandbox`, `cargo run -p sandbox` still exits cleanly, full workspace gate.
