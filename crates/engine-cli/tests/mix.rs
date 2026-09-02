use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use assert_cmd::Command;

const MIX_SCENE: &str = "tests/fixtures/scenes/mix_demo.toml";
const GOLDEN: &str = "tests/fixtures/scenes/mix_demo.golden.wav";
const ASSETS_DIR: &str = "tests/fixtures/assets";

fn scratch_wav() -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "engine-cli-mix-test-{}-{n}.wav",
        std::process::id()
    ))
}

fn run_mix(scene: &str, to: &std::path::Path, ticks: &str, sample_rate: &str) {
    Command::cargo_bin("engine")
        .unwrap()
        .args([
            "mix",
            scene,
            "--to",
            to.to_str().unwrap(),
            "--assets-dir",
            ASSETS_DIR,
            "--ticks",
            ticks,
            "--sample-rate",
            sample_rate,
        ])
        .assert()
        .success();
}

/// `engine mix` never opens a real audio device — this is the audio
/// equivalent of `crates/engine-cli/tests/render.rs`'s golden-image tests,
/// except byte-exact (not tolerance-based): mixing is pure arithmetic over
/// already-decoded samples, so there's no cross-machine software-rasterizer
/// drift to accommodate the way `render`'s golden images do (see ADR-0016).
#[test]
fn mix_matches_golden_wav_byte_for_byte() {
    let out = scratch_wav();
    run_mix(MIX_SCENE, &out, "30", "8000");

    let fresh = std::fs::read(&out).unwrap();
    let golden = std::fs::read(GOLDEN).unwrap();
    assert_eq!(
        fresh, golden,
        "mixdown drifted from the golden WAV reference — mixing is pure \
         arithmetic, so this should never happen without an intentional change"
    );
    std::fs::remove_file(&out).ok();
}

#[test]
fn mix_is_byte_identical_across_two_runs_with_the_same_seed_and_ticks() {
    let out_a = scratch_wav();
    let out_b = scratch_wav();
    run_mix(MIX_SCENE, &out_a, "30", "8000");
    run_mix(MIX_SCENE, &out_b, "30", "8000");

    let a = std::fs::read(&out_a).unwrap();
    let b = std::fs::read(&out_b).unwrap();
    assert_eq!(a, b, "same scene/ticks/sample-rate should mix identically");

    std::fs::remove_file(&out_a).ok();
    std::fs::remove_file(&out_b).ok();
}

#[test]
fn mix_produces_only_silence_for_a_scene_with_no_audio_components() {
    let out = scratch_wav();
    run_mix("tests/fixtures/scenes/basic.toml", &out, "10", "8000");

    let bytes = std::fs::read(&out).unwrap();
    // 44-byte canonical WAV header, zero data frames: `basic.toml` has no
    // `AudioSource`/scripted `engine.play_sound` at all, so nothing is ever
    // queued into the `Mixdown` to render.
    assert_eq!(bytes.len(), 44, "expected a header-only (silent) WAV file");

    std::fs::remove_file(&out).ok();
}

#[test]
fn cli_mix_subcommand_reports_structured_json_status() {
    let out = scratch_wav();
    Command::cargo_bin("engine")
        .unwrap()
        .args([
            "mix",
            MIX_SCENE,
            "--to",
            out.to_str().unwrap(),
            "--assets-dir",
            ASSETS_DIR,
            "--ticks",
            "10",
            "--format",
            "json",
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("\"status\":\"ok\""));

    std::fs::remove_file(&out).ok();
}

#[test]
fn cli_mix_rejects_zero_ticks() {
    let out = scratch_wav();
    Command::cargo_bin("engine")
        .unwrap()
        .args([
            "mix",
            MIX_SCENE,
            "--to",
            out.to_str().unwrap(),
            "--assets-dir",
            ASSETS_DIR,
            "--ticks",
            "0",
            "--format",
            "json",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("INVALID_TICKS"));
}
