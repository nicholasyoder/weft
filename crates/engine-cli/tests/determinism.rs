use assert_cmd::Command;
use engine_cli::DeterminismResult;

#[test]
fn basic_scenario_is_deterministic() {
    assert!(engine_cli::verify_scenario_determinism("basic", 42, 100).is_ok());
}

#[test]
fn broken_rng_scenario_is_caught_as_nondeterministic() {
    match engine_cli::verify_scenario_determinism("broken-rng", 1, 10) {
        Err(DeterminismResult::Mismatch(_)) => {}
        Err(DeterminismResult::Error(e)) => panic!("unexpected error: {}", e.message),
        Ok(_) => panic!("harness failed to detect ambient RNG nondeterminism"),
    }
}

#[test]
fn unknown_scenario_is_an_error_not_a_panic() {
    match engine_cli::verify_scenario_determinism("does-not-exist", 1, 10) {
        Err(DeterminismResult::Error(e)) => assert_eq!(e.code, "SCENARIO_NOT_FOUND"),
        _ => panic!("expected SCENARIO_NOT_FOUND error"),
    }
}

#[test]
fn replay_of_fixture_recording_is_byte_identical_across_two_runs() {
    let mut cmd_a = Command::cargo_bin("engine").unwrap();
    let out_a = cmd_a
        .args(["replay", "tests/fixtures/basic.json", "--format", "json"])
        .output()
        .unwrap();

    let mut cmd_b = Command::cargo_bin("engine").unwrap();
    let out_b = cmd_b
        .args(["replay", "tests/fixtures/basic.json", "--format", "json"])
        .output()
        .unwrap();

    assert!(out_a.status.success());
    assert!(out_b.status.success());
    assert_eq!(out_a.stdout, out_b.stdout);
}

#[test]
fn cli_test_subcommand_passes_for_basic_scenario() {
    Command::cargo_bin("engine")
        .unwrap()
        .args(["test", "--scenario", "basic", "--format", "json"])
        .assert()
        .success()
        .stdout(predicates::str::contains("\"status\":\"pass\""));
}

#[test]
fn cli_test_subcommand_fails_for_broken_rng_scenario() {
    Command::cargo_bin("engine")
        .unwrap()
        .args(["test", "--scenario", "broken-rng", "--format", "json"])
        .assert()
        .failure()
        .stdout(predicates::str::contains("\"status\":\"fail\""));
}

#[test]
fn cli_reports_structured_error_for_unknown_scenario() {
    Command::cargo_bin("engine")
        .unwrap()
        .args(["test", "--scenario", "nope", "--format", "json"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("SCENARIO_NOT_FOUND"));
}

#[test]
fn cli_reports_structured_error_for_missing_recording_file() {
    Command::cargo_bin("engine")
        .unwrap()
        .args([
            "replay",
            "tests/fixtures/does-not-exist.json",
            "--format",
            "json",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("RECORDING_READ_ERROR"));
}

#[test]
fn cli_rejects_zero_ticks() {
    Command::cargo_bin("engine")
        .unwrap()
        .args([
            "test",
            "--scenario",
            "basic",
            "--ticks",
            "0",
            "--format",
            "json",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("INVALID_TICKS"));
}
