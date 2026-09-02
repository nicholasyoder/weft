//! Subprocess-driven integration test: spawns the real `engine-mcp` binary
//! over stdio (not an in-process transport) and drives it with `rmcp`'s own
//! client machinery, the same "speak the real protocol to a real process"
//! posture `engine-cli/tests/watch.rs` already uses for `engine run
//! --watch`. Exercises every one of the seven tools once against a fixture
//! (reusing `engine-cli`'s own fixtures rather than duplicating them) plus
//! one deliberately bad input, turning Phase 5's own DoD — "an agent can
//! build a trivial scene, run it, inspect its state, and diagnose a
//! deliberately introduced bug using only the CLI/MCP surface" — into a
//! permanent regression test.

use rmcp::model::CallToolRequestParams;
use rmcp::transport::TokioChildProcess;
use rmcp::ServiceExt;
use serde_json::{json, Map, Value};
use tokio::process::Command;

const RUN_SCENE: &str = "../engine-cli/tests/fixtures/scenes/basic.toml";
const RENDER_SCENE: &str = "../engine-cli/tests/fixtures/scenes/render_basic.toml";
// Deliberately the no-`Script` mix fixture: `Script.path` has no
// scene-relative resolution (it's resolved against the *process*'s CWD),
// so a scripted fixture would only work from `engine-cli`'s own CWD, not
// this crate's — see `games/sandbox/src/main.rs`'s doc comment for the
// same gotcha in a different caller. `engine-cli/tests/mix.rs` already
// covers the scripted one-shot path from its own CWD; this only needs to
// prove `weft_mix` itself works end-to-end.
const MIX_SCENE: &str = "../engine-cli/tests/fixtures/scenes/mix_no_script.toml";
const MIX_ASSETS_DIR: &str = "../engine-cli/tests/fixtures/assets";
const RECORDING: &str = "../engine-cli/tests/fixtures/basic.json";
const GLTF: &str = "../engine-cli/tests/fixtures/gltf/box_textured.gltf";

fn args(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        other => panic!("expected a JSON object, got {other}"),
    }
}

fn scratch_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("engine-mcp-test-{}-{name}", std::process::id()))
}

async fn connect() -> rmcp::service::RunningService<rmcp::RoleClient, ()> {
    let transport = TokioChildProcess::new(Command::new(env!("CARGO_BIN_EXE_engine-mcp")))
        .expect("failed to spawn engine-mcp");
    ().serve(transport)
        .await
        .expect("failed to complete MCP handshake with engine-mcp")
}

#[tokio::test]
async fn every_tool_succeeds_against_a_fixture() {
    let client = connect().await;

    let run = client
        .call_tool(
            CallToolRequestParams::new("weft_run")
                .with_arguments(args(json!({ "scene": RUN_SCENE, "ticks": 5 }))),
        )
        .await
        .unwrap();
    assert_ne!(run.is_error, Some(true), "weft_run: {run:?}");
    assert_eq!(run.structured_content.unwrap()["status"], "ok");

    let test = client
        .call_tool(
            CallToolRequestParams::new("weft_test")
                .with_arguments(args(json!({ "scenario": "basic" }))),
        )
        .await
        .unwrap();
    assert_ne!(test.is_error, Some(true), "weft_test: {test:?}");
    assert_eq!(test.structured_content.unwrap()["status"], "pass");

    let inspect = client
        .call_tool(
            CallToolRequestParams::new("weft_inspect")
                .with_arguments(args(json!({ "recording": RECORDING }))),
        )
        .await
        .unwrap();
    assert_ne!(inspect.is_error, Some(true), "weft_inspect: {inspect:?}");
    assert!(inspect.structured_content.unwrap()["entities"].is_array());

    let replay = client
        .call_tool(
            CallToolRequestParams::new("weft_replay")
                .with_arguments(args(json!({ "recording": RECORDING }))),
        )
        .await
        .unwrap();
    assert_ne!(replay.is_error, Some(true), "weft_replay: {replay:?}");
    // The fixture recording sets `dump_every`, so replay returns snapshots
    // rather than a single final `world`.
    assert!(replay.structured_content.unwrap()["snapshots"].is_array());

    let render_to = scratch_path("render.png");
    let render = client
        .call_tool(
            CallToolRequestParams::new("weft_render").with_arguments(args(json!({
                "scene": RENDER_SCENE,
                "to": render_to.to_str().unwrap(),
                "width": 32,
                "height": 32,
            }))),
        )
        .await
        .unwrap();
    assert_ne!(render.is_error, Some(true), "weft_render: {render:?}");
    assert_eq!(render.structured_content.unwrap()["status"], "ok");
    assert!(render_to.exists(), "weft_render should have written a PNG");
    std::fs::remove_file(&render_to).ok();

    let mix_to = scratch_path("mix.wav");
    let mix = client
        .call_tool(
            CallToolRequestParams::new("weft_mix").with_arguments(args(json!({
                "scene": MIX_SCENE,
                "to": mix_to.to_str().unwrap(),
                "assets_dir": MIX_ASSETS_DIR,
                "ticks": 10,
            }))),
        )
        .await
        .unwrap();
    assert_ne!(mix.is_error, Some(true), "weft_mix: {mix:?}");
    assert_eq!(mix.structured_content.unwrap()["status"], "ok");
    assert!(mix_to.exists(), "weft_mix should have written a WAV file");
    std::fs::remove_file(&mix_to).ok();

    let import_assets_dir = scratch_path("import-assets");
    let import = client
        .call_tool(
            CallToolRequestParams::new("weft_import").with_arguments(args(json!({
                "input": GLTF,
                "assets_dir": import_assets_dir.to_str().unwrap(),
            }))),
        )
        .await
        .unwrap();
    assert_ne!(import.is_error, Some(true), "weft_import: {import:?}");
    let import_content = import.structured_content.unwrap();
    assert_eq!(import_content["status"], "ok");
    assert!(import_content["fragment"]
        .as_str()
        .unwrap()
        .contains("MeshRef"));
    std::fs::remove_dir_all(&import_assets_dir).ok();

    client.cancel().await.ok();
}

/// Asserts `call_tool(name, arguments)` fails with the given structured
/// `error.code`, on a caller-supplied client (so callers can share one
/// connection across several assertions instead of spawning a subprocess
/// per case).
async fn assert_tool_error(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
    name: &'static str,
    arguments: Value,
    expected_code: &str,
) {
    let result = client
        .call_tool(CallToolRequestParams::new(name).with_arguments(args(arguments)))
        .await
        .unwrap();
    assert_eq!(result.is_error, Some(true), "{name}: {result:?}");
    assert_eq!(
        result.structured_content.unwrap()["error"]["code"],
        expected_code,
        "{name}"
    );
}

#[tokio::test]
async fn structured_errors_survive_the_mcp_boundary() {
    let client = connect().await;

    assert_tool_error(
        &client,
        "weft_run",
        json!({ "scene": "does-not-exist.toml" }),
        "SCENE_READ_ERROR",
    )
    .await;
    assert_tool_error(
        &client,
        "weft_test",
        json!({ "scenario": "does-not-exist" }),
        "SCENARIO_NOT_FOUND",
    )
    .await;

    // A scenario/scene pair given together conflicts the same way for any
    // tool that accepts both — `weft_test` is representative.
    assert_tool_error(
        &client,
        "weft_test",
        json!({ "scenario": "basic", "scene": RUN_SCENE }),
        "SIM_SOURCE_CONFLICT",
    )
    .await;

    // `ticks: 0` is rejected identically by every tool that accepts ticks —
    // cheap enough to check all five rather than just one representative.
    assert_tool_error(
        &client,
        "weft_run",
        json!({ "scene": RUN_SCENE, "ticks": 0 }),
        "INVALID_TICKS",
    )
    .await;
    assert_tool_error(
        &client,
        "weft_test",
        json!({ "scenario": "basic", "ticks": 0 }),
        "INVALID_TICKS",
    )
    .await;
    assert_tool_error(
        &client,
        "weft_inspect",
        json!({ "scenario": "basic", "ticks": 0 }),
        "INVALID_TICKS",
    )
    .await;
    assert_tool_error(
        &client,
        "weft_render",
        json!({ "scene": RENDER_SCENE, "to": scratch_path("invalid-ticks.png").to_str().unwrap(), "ticks": 0 }),
        "INVALID_TICKS",
    )
    .await;
    assert_tool_error(
        &client,
        "weft_mix",
        json!({ "scene": MIX_SCENE, "to": scratch_path("invalid-ticks.wav").to_str().unwrap(), "ticks": 0 }),
        "INVALID_TICKS",
    )
    .await;

    // One error path each for the four tools the above cases don't touch,
    // reusing fixtures `engine-cli`'s own test suite already established
    // for the same underlying errors.
    assert_tool_error(
        &client,
        "weft_replay",
        json!({ "recording": "../engine-cli/tests/fixtures/recording_invalid_source.json" }),
        "RECORDING_INVALID_SOURCE",
    )
    .await;
    // `RUN_SCENE` (basic.toml) has no Camera entity — same fixture
    // `engine-cli/tests/render.rs`'s no-camera test uses.
    assert_tool_error(
        &client,
        "weft_render",
        json!({ "scene": RUN_SCENE, "to": scratch_path("no-camera.png").to_str().unwrap() }),
        "RENDER_NO_CAMERA",
    )
    .await;
    assert_tool_error(
        &client,
        "weft_mix",
        json!({ "scene": "does-not-exist.toml", "to": scratch_path("no-scene.wav").to_str().unwrap() }),
        "SCENE_READ_ERROR",
    )
    .await;
    assert_tool_error(
        &client,
        "weft_import",
        json!({ "input": "Cargo.toml", "assets_dir": scratch_path("import-unsupported").to_str().unwrap() }),
        "IMPORT_UNSUPPORTED_EXTENSION",
    )
    .await;

    client.cancel().await.ok();
}

/// Probes what happens when `rmcp`'s own JSON-schema deserialization
/// rejects a call *before* a tool body runs — `RunParams.scene` is
/// required, so an empty arguments object never reaches `WeftServer::run`
/// at all. This bypasses `CliError` entirely; AGENTS.md calls an
/// unobserved case here "a bug worth filing," so this test exists to pin
/// down and assert on whatever the actual current behavior is.
#[tokio::test]
async fn missing_required_field_is_rejected_before_the_tool_body_runs() {
    let client = connect().await;

    let result = client
        .call_tool(CallToolRequestParams::new("weft_run").with_arguments(args(json!({}))))
        .await
        .unwrap();

    // Confirmed by running this test: rmcp's `Parameters<T>` extractor
    // rejects this itself, before `WeftServer::run` ever executes, as a
    // tool-level result (not a protocol-level `Err` — `call_tool` above
    // still returns `Ok`). It fails loudly (`is_error: Some(true)`, a
    // human-readable "missing field `scene`" message) rather than silently
    // no-oping or panicking, so it satisfies AGENTS.md's core "fails
    // loudly" goal. But its shape doesn't match this crate's own
    // documented contract (the module doc comment's "error payload is
    // always `{\"error\": {code, message, context}}`"): there is no
    // `structured_content` at all here, just a plain-text `content`
    // message with no `error.code` a caller could match on. Asserting on
    // this pinned, real gap rather than papering over it — see
    // known-issues.md.
    assert_eq!(result.is_error, Some(true), "{result:?}");
    assert!(
        result.structured_content.is_none(),
        "expected no structured_content for a pre-tool-body schema rejection, got: {result:?}"
    );
    let text = result.content.first().and_then(|c| c.as_text()).unwrap();
    assert!(
        text.text.contains("scene"),
        "expected the rejection message to mention the missing field, got: {}",
        text.text
    );

    client.cancel().await.ok();
}
