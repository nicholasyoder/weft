# Known issues & engineering debt

> **This is a different kind of list than the tier files.** Tiers 1–4 track *capabilities the engine doesn't have yet*. This file tracks *places where the code doesn't do what its own docs/tests/design already claim it does*, plus architectural debt that's cheap to fix now and expensive to fix once more is built on top of it. Living list, not a queue — pull whatever's actually biting. Fixed items are removed on the spot rather than kept as a changelog; `git log` is the record of what was fixed and why.

---

## Architectural debt worth addressing before it compounds

- **`Resources` (`engine-core/src/resources.rs`) is an `Option`-typed grab-bag** with no compile-time distinction between "always present" (e.g. `AudioSettings`) and "genuinely optional" (e.g. `AssetsDir`) resources — that distinction lives only in doc comments. A heavier required/optional struct split was considered and deliberately deferred; `Resources::remove::<T>()` already closes the sharpest edge (a resource no longer has to live for the `Sim`'s full lifetime).
- **The scenario/scene split is a widening seam, not a static one.** `SimSource::build` (`engine-cli/src/lib.rs:56-75`) manually re-inserts defaults (e.g. `AudioSettings::default()`) for the hardcoded-`Scenario` arm because it never goes through `engine_scene::load`. Every new scene-file-only feature needs a parallel manual patch to keep both paths equivalent. Only 1 of 4 built-in scenarios (`broken-rng`) actually lacks a scene-file counterpart, by design — it exists to bypass the seeded RNG via `rand::thread_rng()`, which can't be reproduced from a declarative scene file.
- **`engine-render/src/gpu.rs`'s `draw()` allocates a fresh uniform buffer + bind group per drawable every frame — no pooling.** Deliberately deferred so far; no perf test exists yet to validate pooling against. Worth revisiting once Tier 2/3's fourth render pass (shadows) lands, since that's when the per-frame allocation cost starts compounding.

---

## Process / testing gaps

- **A malformed/missing-required-field MCP call fails outside the `{code, message, context}` envelope.** `rmcp`'s own JSON-schema rejection (before a tool body runs) surfaces as `is_error: true` with a readable message, but never goes through `CliError` — no `structured_content`, no `error.code`, unlike every other failure path in this codebase. Not a crash, just an undocumented exception to `AGENTS.md`'s "every failure is structured" claim. Worth either wrapping it into the same envelope or documenting the exception explicitly.
