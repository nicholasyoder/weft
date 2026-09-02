//! A thin `rmcp`-based MCP server exposing the seven `engine` CLI commands
//! (`run`/`test`/`inspect`/`replay`/`render`/`mix`/`import`) as typed tools —
//! see ADR-0007 for why every tool body here is just arg-shaping around
//! `engine_cli`'s public core functions, never a second implementation of
//! any command's logic.
//!
//! Every tool returns a *tool-level* result (`CallToolResult::structured`
//! on success, `CallToolResult::structured_error` on failure), never an
//! MCP-protocol-level `ErrorData` — a domain failure like "unknown
//! scenario" is something the calling agent should see and reason about,
//! not have hidden behind an opaque protocol error (see the doc comment on
//! `rmcp::model::CallToolResult::error` for the distinction this follows).
//! The error payload is always `{"error": {code, message, context}}`, the
//! exact `Serialize` shape of `engine_cli::diagnostics::CliError` — the
//! same envelope `engine`'s own `--format json` mode prints to stderr, so
//! the diagnostics contract is identical on both surfaces.

use engine_cli::diagnostics::CliError;
use engine_cli::recording::Recording;
use engine_cli::SimSource;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ServerCapabilities, ServerInfo};
use rmcp::{schemars, tool, tool_handler, tool_router, ServerHandler};
use serde::Deserialize;
use serde_json::json;

fn ok(value: serde_json::Value) -> CallToolResult {
    CallToolResult::structured(value)
}

fn err(e: CliError) -> CallToolResult {
    CallToolResult::structured_error(json!({ "error": e }))
}

/// Resolves a `scenario`/`scene` pair the way `test`/`inspect`'s CLI clap
/// groups do: at most one may be given, and if neither is, `"basic"` is the
/// default scenario — mirrors `main.rs`'s inline handling for those two
/// commands (there's no clap here to enforce it structurally, so the check
/// is explicit).
fn sim_source(scenario: Option<String>, scene: Option<String>) -> Result<SimSource, CliError> {
    match (scenario, scene) {
        (Some(_), Some(_)) => Err(CliError::new(
            "SIM_SOURCE_CONFLICT",
            "specify at most one of `scenario` or `scene`",
        )),
        (_, Some(scene)) => Ok(SimSource::Scene(scene.into())),
        (scenario, None) => Ok(SimSource::Scenario(
            scenario.unwrap_or_else(|| "basic".to_string()),
        )),
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RunParams {
    /// Path to the scene file to load and run.
    pub scene: String,
    /// Seed for the deterministic RNG. Defaults to 1.
    pub seed: Option<u64>,
    /// Number of ticks to run. Defaults to 60.
    pub ticks: Option<u64>,
    /// Content-addressed asset store directory. Defaults to "assets" — only
    /// matters if the scene uses `Animator`/asset-referencing components.
    pub assets_dir: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TestParams {
    /// Name of a built-in scenario (e.g. "basic"). Ignored if `scene` is set;
    /// defaults to "basic" if neither is given.
    pub scenario: Option<String>,
    /// Path to a scene file. Conflicts with `scenario`.
    pub scene: Option<String>,
    pub seed: Option<u64>,
    pub ticks: Option<u64>,
    /// Content-addressed asset store directory. Defaults to "assets".
    pub assets_dir: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct InspectParams {
    /// Name of a built-in scenario. Defaults to "basic" if none of
    /// `scenario`/`scene`/`recording` is given.
    pub scenario: Option<String>,
    /// Path to a scene file. Conflicts with `scenario` and `recording`.
    pub scene: Option<String>,
    /// Path to a recording file. Conflicts with `scenario` and `scene`.
    pub recording: Option<String>,
    pub seed: Option<u64>,
    pub ticks: Option<u64>,
    /// Content-addressed asset store directory. Defaults to "assets".
    pub assets_dir: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ReplayParams {
    /// Path to the recording file to replay.
    pub recording: String,
    /// Content-addressed asset store directory. Defaults to "assets".
    pub assets_dir: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RenderParams {
    /// Path to the scene file to load, run, and render.
    pub scene: String,
    /// Path to write the rendered PNG to.
    pub to: String,
    /// Content-addressed asset store directory. Defaults to "assets".
    pub assets_dir: Option<String>,
    pub seed: Option<u64>,
    pub ticks: Option<u64>,
    /// Output image width in pixels. Defaults to 256.
    pub width: Option<u32>,
    /// Output image height in pixels. Defaults to 256.
    pub height: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MixParams {
    /// Path to the scene file to load, run, and mix down to a WAV file.
    pub scene: String,
    /// Path to write the mixed-down WAV file to.
    pub to: String,
    /// Content-addressed asset store directory. Defaults to "assets".
    pub assets_dir: Option<String>,
    pub seed: Option<u64>,
    pub ticks: Option<u64>,
    /// Output sample rate in Hz. Defaults to 44100.
    pub sample_rate: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ImportParams {
    /// Path to a glTF file (.gltf/.glb), a loose image file, a font
    /// (.ttf/.otf), or an audio file (.wav/.ogg) to import.
    pub input: String,
    /// Content-addressed asset store directory. Defaults to "assets".
    pub assets_dir: Option<String>,
}

#[derive(Debug, Clone)]
pub struct WeftServer {
    tool_router: ToolRouter<Self>,
}

impl Default for WeftServer {
    fn default() -> Self {
        Self::new()
    }
}

impl WeftServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_router]
impl WeftServer {
    /// Loads a scene file, runs it for `ticks` ticks, and returns the final
    /// world state — the same operation as `engine run`.
    #[tool(
        name = "weft_run",
        description = "Load a scene file, run it for N ticks, and return the final world state."
    )]
    async fn run(&self, Parameters(p): Parameters<RunParams>) -> CallToolResult {
        let seed = p.seed.unwrap_or(1);
        let ticks = p.ticks.unwrap_or(60);
        if ticks == 0 {
            return err(CliError::invalid_ticks(ticks));
        }
        let assets_dir = p.assets_dir.unwrap_or_else(|| "assets".to_string());
        match engine_cli::run_and_dump_with_assets_dir(
            SimSource::Scene(p.scene.clone().into()),
            seed,
            ticks,
            assets_dir.as_ref(),
        ) {
            Ok(world) => ok(json!({
                "status": "ok",
                "scene": p.scene,
                "seed": seed,
                "ticks": ticks,
                "world": world,
            })),
            Err(e) => err(e),
        }
    }

    /// Runs a scenario or scene twice and verifies it produces byte-identical
    /// world state — the same operation as `engine test`.
    #[tool(
        name = "weft_test",
        description = "Run a scenario or scene twice and verify it is deterministic."
    )]
    async fn test(&self, Parameters(p): Parameters<TestParams>) -> CallToolResult {
        let seed = p.seed.unwrap_or(1);
        let ticks = p.ticks.unwrap_or(60);
        if ticks == 0 {
            return err(CliError::invalid_ticks(ticks));
        }
        let source = match sim_source(p.scenario, p.scene) {
            Ok(source) => source,
            Err(e) => return err(e),
        };
        let label = source.label();
        let assets_dir = p.assets_dir.unwrap_or_else(|| "assets".to_string());
        match engine_cli::verify_scenario_determinism_with_assets_dir(
            source,
            seed,
            ticks,
            assets_dir.as_ref(),
        ) {
            Ok(json) => ok(json!({
                "status": "pass",
                "source": label,
                "seed": seed,
                "ticks": ticks,
                "world": json,
            })),
            Err(engine_cli::DeterminismResult::Error(e)) => err(e),
            Err(engine_cli::DeterminismResult::Mismatch(fail)) => ok(json!({
                "status": "fail",
                "source": fail.source,
                "reason": "nondeterministic",
                "run_a": fail.json_a,
                "run_b": fail.json_b,
            })),
        }
    }

    /// Dumps a scenario's, scene's, or recording's final world state as
    /// JSON — the same operation as `engine inspect`.
    #[tool(
        name = "weft_inspect",
        description = "Dump a scenario's, scene's, or recording's final world state as JSON."
    )]
    async fn inspect(&self, Parameters(p): Parameters<InspectParams>) -> CallToolResult {
        let (source, seed, ticks) = if let Some(path) = p.recording {
            match Recording::load(path.as_ref()) {
                Ok(r) => {
                    let seed = r.seed;
                    let ticks = r.ticks;
                    (r.source(), seed, ticks)
                }
                Err(e) => return err(e),
            }
        } else {
            let seed = p.seed.unwrap_or(1);
            let ticks = p.ticks.unwrap_or(60);
            let source = match sim_source(p.scenario, p.scene) {
                Ok(source) => source,
                Err(e) => return err(e),
            };
            (source, seed, ticks)
        };

        if ticks == 0 {
            return err(CliError::invalid_ticks(ticks));
        }

        let assets_dir = p.assets_dir.unwrap_or_else(|| "assets".to_string());
        match engine_cli::run_and_dump_with_assets_dir(source, seed, ticks, assets_dir.as_ref()) {
            Ok(world) => ok(world),
            Err(e) => err(e),
        }
    }

    /// Replays a recording file deterministically — the same operation as
    /// `engine replay`.
    #[tool(
        name = "weft_replay",
        description = "Replay a recording file deterministically."
    )]
    async fn replay(&self, Parameters(p): Parameters<ReplayParams>) -> CallToolResult {
        let recording = match Recording::load(p.recording.as_ref()) {
            Ok(r) => r,
            Err(e) => return err(e),
        };
        if recording.ticks == 0 {
            return err(CliError::invalid_ticks(recording.ticks));
        }

        let assets_dir = p.assets_dir.unwrap_or_else(|| "assets".to_string());
        let source = recording.source();
        let result = match recording.dump_every {
            Some(every) if every > 0 => engine_cli::run_and_dump_snapshots_with_assets_dir(
                source,
                recording.seed,
                recording.ticks,
                every,
                assets_dir.as_ref(),
            )
            .map(|snapshots| json!({ "snapshots": snapshots })),
            _ => engine_cli::run_and_dump_with_assets_dir(
                source,
                recording.seed,
                recording.ticks,
                assets_dir.as_ref(),
            )
            .map(|world| json!({ "world": world })),
        };

        match result {
            Ok(json) => ok(json),
            Err(e) => err(e),
        }
    }

    /// Loads a scene file, runs it, and renders the final world state to a
    /// PNG file — the same operation as `engine render`.
    #[tool(
        name = "weft_render",
        description = "Load a scene file, run it, and render the final world state to a PNG file."
    )]
    async fn render(&self, Parameters(p): Parameters<RenderParams>) -> CallToolResult {
        let seed = p.seed.unwrap_or(1);
        let ticks = p.ticks.unwrap_or(60);
        if ticks == 0 {
            return err(CliError::invalid_ticks(ticks));
        }
        let assets_dir = p.assets_dir.unwrap_or_else(|| "assets".to_string());
        let width = p.width.unwrap_or(256);
        let height = p.height.unwrap_or(256);

        match engine_cli::render_scene(
            p.scene.as_ref(),
            seed,
            ticks,
            width,
            height,
            assets_dir.as_ref(),
            p.to.as_ref(),
        ) {
            Ok(()) => ok(json!({
                "status": "ok",
                "scene": p.scene,
                "to": p.to,
                "width": width,
                "height": height,
            })),
            Err(e) => err(e),
        }
    }

    /// Loads a scene file, runs it, and writes the resulting audio mixdown
    /// to a WAV file — the same operation as `engine mix`. No real audio
    /// device is used or required (see ADR-0016).
    #[tool(
        name = "weft_mix",
        description = "Load a scene file, run it, and write the resulting audio mixdown to a WAV file. No real audio device required."
    )]
    async fn mix(&self, Parameters(p): Parameters<MixParams>) -> CallToolResult {
        let seed = p.seed.unwrap_or(1);
        let ticks = p.ticks.unwrap_or(60);
        if ticks == 0 {
            return err(CliError::invalid_ticks(ticks));
        }
        let assets_dir = p.assets_dir.unwrap_or_else(|| "assets".to_string());
        let sample_rate = p.sample_rate.unwrap_or(44100);

        match engine_cli::mix_scene(
            p.scene.as_ref(),
            seed,
            ticks,
            sample_rate,
            assets_dir.as_ref(),
            p.to.as_ref(),
        ) {
            Ok(()) => ok(json!({
                "status": "ok",
                "scene": p.scene,
                "to": p.to,
                "sample_rate": sample_rate,
                "ticks": ticks,
            })),
            Err(e) => err(e),
        }
    }

    /// Converts a glTF file, a loose image file, a font file, or an audio
    /// file into the content-addressed asset store and returns a
    /// pasteable scene-text-file fragment — the same operation as
    /// `engine import`.
    /// Unlike the CLI's optional `--out`, this always returns the fragment
    /// as tool output rather than writing a file (an agent driving this
    /// tool already has filesystem tools of its own if it wants one
    /// written out).
    #[tool(
        name = "weft_import",
        description = "Import a glTF, image, font (.ttf/.otf), or audio (.wav/.ogg) file into the asset store and return a pasteable scene-text-file fragment."
    )]
    async fn import(&self, Parameters(p): Parameters<ImportParams>) -> CallToolResult {
        let assets_dir = p.assets_dir.unwrap_or_else(|| "assets".to_string());
        match engine_cli::import_asset(p.input.as_ref(), assets_dir.as_ref()) {
            Ok(result) => ok(json!({
                "status": "ok",
                "input": p.input,
                "assets_dir": assets_dir,
                "fragment": result.fragment,
                "mesh_hash": result.mesh_hash,
                "texture_hash": result.texture_hash,
                "font_hash": result.font_hash,
                "skin_hash": result.skin_hash,
                "skeleton_hash": result.skeleton_hash,
                "clip_hash": result.clip_hash,
                "audio_hash": result.audio_hash,
            })),
            Err(e) => err(e),
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for WeftServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "Weft engine MCP server. Seven tools (weft_run, weft_test, weft_inspect, \
             weft_replay, weft_render, weft_mix, weft_import) are 1:1 wrappers over the \
             `engine` CLI's run/test/inspect/replay/render/mix/import subcommands, with \
             identical semantics and error shape. Every failure returns a structured \
             {\"error\": {\"code\", \"message\", \"context\"}} payload — see AGENTS.md \
             at the repo root for the full contract.",
        )
    }
}
