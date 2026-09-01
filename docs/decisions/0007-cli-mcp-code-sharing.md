# ADR-0007: `engine-cli`/`engine-mcp` code-sharing shape

- **Status**: accepted
- **Date**: 2026-08-31

## Context

Phase 5 (see [ROADMAP.md](../roadmap/completed-phases.md)) adds `engine-mcp`, a thin MCP server exposing the same operations as the `engine` CLI. `research/03 §7` is explicit that this must be "a direct wrapper around a CLI subcommand or library call... never MCP-only functionality, so there's only one implementation of each operation to keep correct." `test`/`inspect`/`run`/`replay` already satisfy this by construction: each `commands/*.rs::run` is a thin arg-plumbing + print/`ExitCode` wrapper around a shared `engine_cli::lib.rs` function (`run_and_dump`, `verify_scenario_determinism`, `build_sim`) that returns `Result<serde_json::Value, CliError>` with no I/O side effects. `render` and `import` had not yet been pulled apart that way — their build-act-format logic lived directly inside their `commands/*.rs::run` functions, which would have forced `engine-mcp` to either duplicate that logic or call the CLI as a subprocess (parsing stdout back out of a tool meant for humans/scripts, the exact anti-pattern the roadmap's own ground rules warn against elsewhere).

## Decision

**Every CLI operation gets exactly one core function in `engine_cli`'s public lib surface, returning `Result<T, CliError>` with no printing and no `ExitCode`.** `commands/*.rs::run` functions are the only place that prints (`println!`/`eprintln!` via `CliError::print`) and returns `ExitCode`; `engine-mcp`'s tool handlers are the only other caller, and they never touch `commands::*::run` directly — only the shared core fn.

Concretely for this phase: `render_scene(scene, seed, ticks, width, height, assets_dir, to) -> Result<(), CliError>` and `import_asset(input, assets_dir) -> Result<ImportResult, CliError>` were added to `lib.rs`, and `commands/render.rs`/`commands/import.rs` were reduced to call them plus handle `--format`/`ExitCode`/the `--out` file write (which is CLI-specific: `engine-mcp` returns the fragment as tool output directly rather than writing a file, so that one piece of behavior correctly stays out of the shared core). `import`'s small fragment-formatting helpers (`gltf_fragment`/`texture_fragment`) stayed in `commands/import.rs` as `pub(crate)` rather than moving wholesale into `lib.rs` — they're pure string formatting with no I/O, small enough that duplicating the *shape* of where they live isn't worth a bigger move, and `import_asset` already calls them so there's still exactly one implementation.

`engine-mcp` becomes a normal downstream crate: it depends on `engine-cli` as a library (`engine_cli::{run_and_dump, verify_scenario_determinism, build_sim, render_scene, import_asset}`, plus `Recording::load` for `replay`), and each tool handler's only job is: deserialize the MCP call's typed input, call the core fn, and translate `Ok`/`Err` into the tool's success/error content — translating a `CliError` into an MCP tool error that carries the exact same `{code, message, context}` payload the CLI prints, not a re-stringified message. This is the one enforcement point for research/03 §7's "diagnostics survive the CLI→MCP boundary unchanged" requirement.

## Alternatives considered

- **`engine-mcp` shells out to the `engine` binary as a subprocess and parses its `--format json` stdout.** Rejected: an extra process per tool call, a second (string-parsing) failure mode on top of the first, and it would silently start depending on the human-readable/JSON *print* format staying stable rather than the `Result<T, CliError>` *type* staying stable — the type is the more honest contract to depend on.
- **Duplicate `render`/`import`'s logic directly inside `engine-mcp`.** Rejected outright by `research/03 §7`'s explicit "never MCP-only functionality" principle, and by this codebase's own established practice (`run_and_dump` etc. already prove the one-core-fn shape works).
- **Move `gltf_fragment`/`texture_fragment` into `lib.rs` alongside `import_asset`.** Considered for symmetry with `render_scene`, but rejected as unnecessary churn — they're private formatting helpers with a single caller each; `pub(crate)`-from-`commands::import` is a smaller diff and the "one implementation" property is what actually matters, not which file it lives in.

## Consequences

- Every CLI command now follows one shape end to end: `commands/*.rs::run` (args + print + `ExitCode`) → a `lib.rs` core fn (`Result<T, CliError>`, no I/O beyond what the operation itself requires) → the relevant `engine-*` crate. `engine-mcp` slots in as a second, equally thin caller of the same core fns — no new pattern was invented for it.
- `ImportResult` (a new small `pub struct` in `lib.rs`) carries both the formatted fragment text and the raw `mesh_hash`/`texture_hash`, so `engine-mcp`'s `import` tool can return structured hash fields to an agent directly instead of asking it to regex them back out of the fragment string.
- `render`'s `--out`-equivalent behavior diverges between the CLI (writes a file when `--out` is given) and MCP (always returns the fragment as tool output, no file write) — a deliberate, narrow difference; see Decision above.

## Revisit when

- A future command's "thin CLI wrapper" stops being thin (e.g. needs CLI-only interactive behavior an MCP tool call can't sensibly offer) — that's the trigger to reconsider whether every command really wants the same core-fn shape, rather than assuming it forever.
- `engine-mcp` needs a transport other than stdio (e.g. a long-running server multiple agents share) — out of scope for this ADR, which only fixes the code-sharing shape, not the transport.
