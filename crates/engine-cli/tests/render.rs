use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use assert_cmd::Command;

const RENDER_SCENE: &str = "tests/fixtures/scenes/render_basic.toml";
const GOLDEN: &str = "tests/fixtures/scenes/render_basic.golden.png";
const RENDER_IMPORTED_SCENE: &str = "tests/fixtures/scenes/render_imported.toml";
const IMPORTED_GOLDEN: &str = "tests/fixtures/scenes/render_imported.golden.png";
const IMPORTED_ASSETS_DIR: &str = "tests/fixtures/assets";

/// Per-channel tolerance for the golden-image comparison. Not blind byte
/// equality: `lavapipe`/Mesa version drift across machines can shift
/// software-rasterized pixels slightly even for an unchanged scene — the
/// roadmap calls this out explicitly as an accepted trade-off.
const MAX_CHANNEL_DIFF: i16 = 8;

fn scratch_png() -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "engine-cli-render-test-{}-{n}.png",
        std::process::id()
    ))
}

fn images_match_within_tolerance(a: &image::RgbaImage, b: &image::RgbaImage) -> bool {
    if a.dimensions() != b.dimensions() {
        return false;
    }
    a.pixels().zip(b.pixels()).all(|(pa, pb)| {
        pa.0.iter()
            .zip(pb.0.iter())
            .all(|(&ca, &cb)| (ca as i16 - cb as i16).abs() <= MAX_CHANNEL_DIFF)
    })
}

#[test]
fn render_matches_golden_image_within_tolerance() {
    let out = scratch_png();
    Command::cargo_bin("engine")
        .unwrap()
        .env_remove("DISPLAY")
        .args([
            "render",
            RENDER_SCENE,
            "--to",
            out.to_str().unwrap(),
            "--width",
            "64",
            "--height",
            "64",
        ])
        .assert()
        .success();

    let fresh = image::open(&out).unwrap().into_rgba8();
    let golden = image::open(GOLDEN).unwrap().into_rgba8();
    assert!(
        images_match_within_tolerance(&fresh, &golden),
        "rendered image drifted from the golden reference by more than {MAX_CHANNEL_DIFF} per channel"
    );
}

#[test]
fn cli_render_subcommand_works_with_no_display_server() {
    let out = scratch_png();
    Command::cargo_bin("engine")
        .unwrap()
        .env_remove("DISPLAY")
        .args([
            "render",
            RENDER_SCENE,
            "--to",
            out.to_str().unwrap(),
            "--width",
            "32",
            "--height",
            "32",
            "--format",
            "json",
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("\"status\":\"ok\""));

    let image = image::open(&out).unwrap();
    assert_eq!((image.width(), image.height()), (32, 32));
}

#[test]
fn render_of_imported_gltf_asset_matches_golden_image_within_tolerance() {
    let out = scratch_png();
    Command::cargo_bin("engine")
        .unwrap()
        .env_remove("DISPLAY")
        .args([
            "render",
            RENDER_IMPORTED_SCENE,
            "--to",
            out.to_str().unwrap(),
            "--assets-dir",
            IMPORTED_ASSETS_DIR,
            "--width",
            "64",
            "--height",
            "64",
        ])
        .assert()
        .success();

    let fresh = image::open(&out).unwrap().into_rgba8();
    let golden = image::open(IMPORTED_GOLDEN).unwrap().into_rgba8();
    assert!(
        images_match_within_tolerance(&fresh, &golden),
        "rendered image drifted from the golden reference by more than {MAX_CHANNEL_DIFF} per channel"
    );
}

#[test]
fn rendering_a_scene_with_an_unknown_asset_hash_is_a_structured_error() {
    let out = scratch_png();
    Command::cargo_bin("engine")
        .unwrap()
        .env_remove("DISPLAY")
        .args([
            "render",
            RENDER_IMPORTED_SCENE,
            "--to",
            out.to_str().unwrap(),
            "--assets-dir",
            "tests/fixtures/nonexistent-assets-dir",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("RENDER_ASSET_ERROR"));
}

#[test]
fn rendering_a_scenario_with_no_camera_is_a_structured_error() {
    let out = scratch_png();
    // `basic`'s scene-file source has no Camera entity.
    Command::cargo_bin("engine")
        .unwrap()
        .env_remove("DISPLAY")
        .args([
            "render",
            "tests/fixtures/scenes/basic.toml",
            "--to",
            out.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("RENDER_NO_CAMERA"));
}
