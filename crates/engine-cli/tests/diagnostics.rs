//! Audits the CLI's structured-error contract itself (per Phase 5's DoD),
//! rather than any one command's behavior — every other test file already
//! covers command-specific failures with a stderr substring check; this
//! file is the one place that asserts the `--format json` error envelope
//! actually parses as JSON and carries a stable `error.code`, and that the
//! human-format error line always starts with `error[CODE]:`. Failure
//! cases already exercised elsewhere (unknown scenario, missing recording
//! file, zero ticks, no-camera render, unsupported import extension,
//! broken Lua script under `--watch`) are not repeated here — only the
//! envelope-shape assertion and the handful of failure paths those files
//! don't reach yet (malformed TOML, a scene's dangling script reference
//! outside `--watch`, an ambiguous recording, two cameras).

use assert_cmd::Command;

/// Runs `engine` with `args` plus `--format json`, asserts it fails, and
/// returns stderr parsed as JSON — panicking with the raw stderr text if it
/// isn't valid JSON, since a malformed JSON error is exactly the silent
/// "gave the agent zero signal" failure mode this test exists to catch.
fn json_error(args: &[&str]) -> serde_json::Value {
    let mut full_args: Vec<&str> = args.to_vec();
    full_args.extend(["--format", "json"]);
    let output = Command::cargo_bin("engine")
        .unwrap()
        .args(&full_args)
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "expected `engine {}` to fail",
        full_args.join(" ")
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    serde_json::from_str(&stderr).unwrap_or_else(|e| {
        panic!("stderr was not valid JSON ({e}): {stderr}");
    })
}

fn assert_error_code(args: &[&str], expected_code: &str) {
    let value = json_error(args);
    assert_eq!(
        value["error"]["code"],
        expected_code,
        "unexpected error envelope for `engine {}`: {value}",
        args.join(" ")
    );
    assert!(
        value["error"]["message"].is_string(),
        "error envelope missing a string message: {value}"
    );
}

#[test]
fn json_error_envelope_has_stable_code_and_message() {
    assert_error_code(
        &["test", "--scenario", "does-not-exist"],
        "SCENARIO_NOT_FOUND",
    );
}

#[test]
fn human_format_error_uses_error_code_prefix() {
    Command::cargo_bin("engine")
        .unwrap()
        .args(["test", "--scenario", "does-not-exist"])
        .assert()
        .failure()
        .stderr(predicates::str::starts_with("error[SCENARIO_NOT_FOUND]:"));
}

#[test]
fn malformed_scene_toml_is_a_structured_parse_error() {
    assert_error_code(
        &["run", "tests/fixtures/scenes/malformed.toml"],
        "SCENE_PARSE_ERROR",
    );
}

#[test]
fn scene_with_dangling_script_reference_is_a_structured_error_without_watch() {
    assert_error_code(
        &["run", "tests/fixtures/scenes/missing_script.toml"],
        "SCRIPT_READ_ERROR",
    );
}

/// Regression test for known-issues.md's "multi-script dispatch errors are
/// collected, then thrown away": `ScriptHost::dispatch` deliberately
/// gathers every per-entity error in a tick so one bad script doesn't hide
/// another's failure, but `step_and_dispatch_with_input` used to keep only
/// the first (`errors.into_iter().next()`). Both entities in this fixture
/// call an undefined function, so both errors must survive into the
/// envelope's `context.errors`, not just one.
#[test]
fn two_failing_scripts_in_the_same_tick_both_surface_in_the_error_context() {
    let value = json_error(&["run", "tests/fixtures/scenes/two_broken_scripts.toml"]);
    assert_eq!(value["error"]["code"], "SCRIPT_UNKNOWN_FUNCTION");
    let errors = value["error"]["context"]["errors"]
        .as_array()
        .unwrap_or_else(|| panic!("expected context.errors to be an array: {value}"));
    assert_eq!(
        errors.len(),
        2,
        "expected both broken_a's and broken_b's script errors, got: {value}"
    );
}

#[test]
fn ambiguous_recording_source_is_a_structured_error() {
    assert_error_code(
        &["replay", "tests/fixtures/recording_invalid_source.json"],
        "RECORDING_INVALID_SOURCE",
    );
}

#[test]
fn scene_with_two_cameras_is_a_structured_error() {
    let out = std::env::temp_dir().join(format!(
        "engine-cli-diagnostics-two-cameras-{}.png",
        std::process::id()
    ));
    assert_error_code(
        &[
            "render",
            "tests/fixtures/scenes/render_two_cameras.toml",
            "--to",
            out.to_str().unwrap(),
        ],
        "RENDER_MULTIPLE_CAMERAS",
    );
}

#[test]
fn unsupported_import_extension_json_envelope() {
    let dir = std::env::temp_dir().join(format!(
        "engine-cli-diagnostics-import-{}",
        std::process::id()
    ));
    assert_error_code(
        &[
            "import",
            "Cargo.toml",
            "--assets-dir",
            dir.join("assets").to_str().unwrap(),
        ],
        "IMPORT_UNSUPPORTED_EXTENSION",
    );
}
