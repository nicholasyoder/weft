# Agent-facing guide to Weft

This is the CLI/MCP contract for driving the engine — build it, run it, inspect it, diagnose it — without reading engine source. For *why* the engine is shaped this way, see [README.md](README.md), [ROADMAP.md](ROADMAP.md), and `docs/decisions/`.

Two equivalent surfaces exist, both thin wrappers over the same `engine_cli` library code (see [ADR-0007](docs/decisions/0007-cli-mcp-code-sharing.md) — there is exactly one implementation of each operation):

- **`engine`**, a CLI binary (`cargo run -p engine-cli --bin engine --`).
- **`engine-mcp`**, an MCP server over stdio (`cargo run -p engine-mcp --bin engine-mcp`), exposing the same seven operations as typed tools for an MCP-aware agent runtime.

## Building

```
cargo build --workspace
```

`cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --all -- --check` must all stay clean — every phase in [`docs/roadmap/completed-phases.md`](docs/roadmap/completed-phases.md) has held to this gate.

## The seven operations

Each row: CLI subcommand, its args, and the equivalent MCP tool + params. Args not marked required have the CLI's default shown; MCP tool params use the same defaults when omitted.

| CLI | Args | MCP tool | Params |
|---|---|---|---|
| `engine run <scene>` | `--seed 1`, `--ticks 60`, `--watch`, `--format human\|json` | `weft_run` | `scene` (required), `seed`, `ticks` |
| `engine test [--scenario \| --scene]` | `--scenario` (default `basic`) or `--scene`, `--seed 1`, `--ticks 60`, `--format` | `weft_test` | `scenario` or `scene` (at most one; defaults to `"basic"`), `seed`, `ticks` |
| `engine inspect [--scenario \| --scene \| --recording]` | same as `test`, plus `--recording` | `weft_inspect` | `scenario`/`scene`/`recording` (at most one), `seed`, `ticks` |
| `engine replay <recording>` | `--format` | `weft_replay` | `recording` (required) |
| `engine render <scene> --to <file.png>` | `--to` (required), `--assets-dir assets`, `--seed 1`, `--ticks 60`, `--width 256`, `--height 256`, `--format` | `weft_render` | `scene`, `to` (both required), `assets_dir`, `seed`, `ticks`, `width`, `height` |
| `engine mix <scene> --to <file.wav>` | `--to` (required), `--assets-dir assets`, `--seed 1`, `--ticks 60`, `--sample-rate 44100`, `--format` | `weft_mix` | `scene`, `to` (both required), `assets_dir`, `seed`, `ticks`, `sample_rate` |
| `engine import <file>` | `--assets-dir assets`, `--out <path>` (optional — writes the fragment to a file), `--format` | `weft_import` | `input` (required), `assets_dir` |

Two deliberate differences, not omissions:

- **No `--watch`, no `--format` on the MCP side.** `--watch` is a long-running human/dev-loop feature; MCP tool calls are one-shot request/response. Every MCP tool always returns structured JSON — there's no human-text mode to select.
- **`weft_import` never writes a file.** The CLI's `--out` is optional convenience for humans; an agent driving `weft_import` already has filesystem tools of its own if it wants the fragment saved. The tool always returns `fragment` (the pasteable scene-text-file snippet) plus `mesh_hash`/`texture_hash`/`font_hash`/`audio_hash` directly in its response.

`run`/`render`/`mix` only ever take a scene file (no `--scenario`); `test`/`inspect` accept either a built-in scenario name or a scene file, defaulting to the `basic` scenario if neither is given.

`engine mix`/`weft_mix` never open a real audio device — the same offline/deterministic posture `render`/`weft_render` already has for graphics (see ADR-0016). Only `engine play` (CLI-only, no MCP tool — see below) touches a live device, and does so gracefully: no device available just means no sound, not a crash.

## Output shape

**CLI, `--format json`** (default is human-readable text): one JSON object printed to stdout on success, e.g. `engine run`:
```json
{"status": "ok", "scene": "...", "seed": 1, "ticks": 60, "world": {"tick": 60, "seed": 1, "entities": [...]}}
```
`inspect` and `replay` print the world/recording dump directly, with no `status` wrapper.

**MCP**: each tool's `structuredContent` is exactly that same JSON object (`CallToolResult::structured`) — an agent reading a `weft_run` response sees the identical shape a CLI `--format json` caller would.

## Errors — the one contract that matters most

Every failure, on both surfaces, is `{"code": "STABLE_UPPER_SNAKE_CODE", "message": "human-readable sentence", "context": {...}}` (`context` may be `null`). This is `engine_cli::diagnostics::CliError`'s `Serialize` shape — see `crates/engine-cli/src/diagnostics.rs` for the full constructor list, and each `engine-*` crate's `src/error.rs` for the domain-specific codes it wraps (`SCENE_*`, `RENDER_*`, `ASSET_*`, `SCRIPT_*`, `IMPORT_*`, `RECORDING_*`, plus CLI-level ones like `SCENARIO_NOT_FOUND`/`INVALID_TICKS`). Codes are stable and meant to be pattern-matched on, not just read.

- **CLI**: non-zero exit code; the error is always on **stderr**, regardless of `--format`. `--format json` prints `{"error": {...}}`; the default human format prints `error[CODE]: message`.
- **MCP**: `isError: true` with `structuredContent: {"error": {...}}` — the identical envelope, reached via `CallToolResult::structured_error`, never a raw MCP protocol-level error. A domain failure like an unknown scenario is something the calling agent is meant to see and act on, not something hidden behind an opaque JSON-RPC error (see the doc comment on `CallToolResult::error` in `rmcp` for the distinction, and [ADR-0007](docs/decisions/0007-cli-mcp-code-sharing.md)).

A request that can't be satisfied always fails loudly with one of these — no command silently no-ops on bad input (this is a first-class design goal, not incidental; see `research/03 §7`). If you hit a failure mode that *doesn't* produce a structured `{code, message, context}` error on either surface, that's a bug worth filing/fixing, not a case to work around.

## Determinism

Every operation is a pure function of `(seed, scene-or-scenario, referenced .lua scripts)` — same seed, same output, byte-for-byte. `engine test`/`weft_test` exist specifically to assert this: they run the same source twice and diff the JSON. If you're debugging "why did this scene produce X," reproduce it with `inspect`/`weft_inspect` at a fixed seed rather than assuming any hidden state.

## Watching a scene/script while iterating

`engine run <scene> --watch` reruns the full `--ticks` budget from scratch on every save to the scene file or any `.lua` file it references (one file-watch mechanism for both), printing one JSON event per run/reload plus a `{"event": "watching"}` line once the watcher is actually armed. It never exits or crashes on a bad edit — a broken script produces the same structured `{status: "error", code: "..."}` shape and the process keeps watching. This is a CLI-only, long-running mode; there is no MCP equivalent (see above) — an agent that wants the same loop calls `weft_run` again after each edit.

## Live windowed play (CLI-only)

`engine play <scene> [--seed 1] [--assets-dir assets] [--width 1024] [--height 768] [--max-ticks N] [--format]` opens a real window and runs the scene live: a wall-clock-paced fixed-timestep loop, not a fixed batch of ticks, reading live keyboard state (the full `KeyCode` set — A–Z, digits, arrows, Space/Enter/Tab/Escape, left/right Shift/Control/Alt, see `engine_core::Input`/`KeyCode`, generalized in [ADR-0013](docs/decisions/0013-live-script-input-and-generalized-keycode.md)) into `Resources` every tick (see [ADR-0010](docs/decisions/0010-live-input-and-windowed-run-loop.md)). Escape or closing the window exits cleanly (exit code 0); `--max-ticks` auto-exits after N sim ticks with no human input, which is what makes this testable at all (see `crates/engine-cli/tests/play.rs`).

Like `--watch`, this is CLI-only — no MCP tool (`play` opens a window and blocks on a live event loop; it doesn't fit MCP's one-shot request/response shape). It needs a real windowing backend (X11/Wayland/etc., e.g. via `Xvfb`) to create a window at all — unlike `engine render`, `Backends::VULKAN` + Mesa's lavapipe alone isn't sufficient (see ADR-0004's headless note, which that command's offscreen-only path doesn't share).

`play` makes no determinism claim of its own beyond what `Sim::step()` already guarantees (each tick is still a pure function of its inputs) — the wall-clock accumulator pacing around it, and live human keyboard input, are both inherently non-reproducible. Recording/replaying a `play` session deterministically is explicitly out of scope for now (see ADR-0010's "revisit when").
