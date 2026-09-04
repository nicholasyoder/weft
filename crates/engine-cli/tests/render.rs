use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use assert_cmd::Command;

const RENDER_SCENE: &str = "tests/fixtures/scenes/render_basic.toml";
const GOLDEN: &str = "tests/fixtures/scenes/render_basic.golden.png";
const RENDER_IMPORTED_SCENE: &str = "tests/fixtures/scenes/render_imported.toml";
const IMPORTED_GOLDEN: &str = "tests/fixtures/scenes/render_imported.golden.png";
const IMPORTED_ASSETS_DIR: &str = "tests/fixtures/assets";
const RENDER_TEXT_SCENE: &str = "tests/fixtures/scenes/render_text.toml";
const TEXT_GOLDEN: &str = "tests/fixtures/scenes/render_text.golden.png";
const ANIMATION_SCENE: &str = "tests/fixtures/scenes/animation_demo.toml";
const ANIMATION_GOLDEN: &str = "tests/fixtures/scenes/render_animation.golden.png";
const ANIMATION_ASSETS_DIR: &str = "tests/fixtures/assets";
const PBR_TEXTURE_SCENE: &str = "tests/fixtures/scenes/render_pbr_texture.toml";
const PBR_TEXTURE_GOLDEN: &str = "tests/fixtures/scenes/render_pbr_texture.golden.png";
const PBR_TEXTURE_ASSETS_DIR: &str = "tests/fixtures/assets";
const NORMAL_MAP_SCENE: &str = "tests/fixtures/scenes/render_normal_map.toml";
const NORMAL_MAP_GOLDEN: &str = "tests/fixtures/scenes/render_normal_map.golden.png";
const NORMAL_MAP_ASSETS_DIR: &str = "tests/fixtures/assets";
const MULTI_LIGHT_SCENE: &str = "tests/fixtures/scenes/render_multi_light.toml";
const MULTI_LIGHT_GOLDEN: &str = "tests/fixtures/scenes/render_multi_light.golden.png";
const SHADOW_SCENE: &str = "tests/fixtures/scenes/render_shadow.toml";
const SHADOW_GOLDEN: &str = "tests/fixtures/scenes/render_shadow.golden.png";

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
fn render_of_text_over_a_3d_scene_matches_golden_image_within_tolerance() {
    let out = scratch_png();
    Command::cargo_bin("engine")
        .unwrap()
        .env_remove("DISPLAY")
        .args([
            "render",
            RENDER_TEXT_SCENE,
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
    let golden = image::open(TEXT_GOLDEN).unwrap().into_rgba8();
    assert!(
        images_match_within_tolerance(&fresh, &golden),
        "rendered image drifted from the golden reference by more than {MAX_CHANNEL_DIFF} per channel"
    );
}

/// The concrete GPU-skinning-through-the-real-binary proof: `animation_demo`
/// run for 30 ticks (0.5s at the default timestep) puts its forearm joint at
/// 45°, visibly bent compared to its bind pose — see
/// `crates/engine-cli/tests/animation.rs` for the matching numeric
/// `JointPalette` assertion this golden image is the rendered counterpart
/// of.
#[test]
fn render_of_a_metallic_roughness_textured_mesh_matches_golden_image_within_tolerance() {
    let out = scratch_png();
    Command::cargo_bin("engine")
        .unwrap()
        .env_remove("DISPLAY")
        .args([
            "render",
            PBR_TEXTURE_SCENE,
            "--to",
            out.to_str().unwrap(),
            "--assets-dir",
            PBR_TEXTURE_ASSETS_DIR,
            "--width",
            "64",
            "--height",
            "64",
        ])
        .assert()
        .success();

    let fresh = image::open(&out).unwrap().into_rgba8();
    let golden = image::open(PBR_TEXTURE_GOLDEN).unwrap().into_rgba8();
    assert!(
        images_match_within_tolerance(&fresh, &golden),
        "rendered image drifted from the golden reference by more than {MAX_CHANNEL_DIFF} per channel"
    );
}

#[test]
fn render_of_a_normal_mapped_mesh_matches_golden_image_within_tolerance() {
    let out = scratch_png();
    Command::cargo_bin("engine")
        .unwrap()
        .env_remove("DISPLAY")
        .args([
            "render",
            NORMAL_MAP_SCENE,
            "--to",
            out.to_str().unwrap(),
            "--assets-dir",
            NORMAL_MAP_ASSETS_DIR,
            "--width",
            "64",
            "--height",
            "64",
        ])
        .assert()
        .success();

    let fresh = image::open(&out).unwrap().into_rgba8();
    let golden = image::open(NORMAL_MAP_GOLDEN).unwrap().into_rgba8();
    assert!(
        images_match_within_tolerance(&fresh, &golden),
        "rendered image drifted from the golden reference by more than {MAX_CHANNEL_DIFF} per channel"
    );
}

#[test]
fn render_of_a_skinned_animated_mesh_matches_golden_image_within_tolerance() {
    let out = scratch_png();
    Command::cargo_bin("engine")
        .unwrap()
        .env_remove("DISPLAY")
        .args([
            "render",
            ANIMATION_SCENE,
            "--to",
            out.to_str().unwrap(),
            "--assets-dir",
            ANIMATION_ASSETS_DIR,
            "--ticks",
            "30",
            "--width",
            "64",
            "--height",
            "64",
        ])
        .assert()
        .success();

    let fresh = image::open(&out).unwrap().into_rgba8();
    let golden = image::open(ANIMATION_GOLDEN).unwrap().into_rgba8();
    assert!(
        images_match_within_tolerance(&fresh, &golden),
        "rendered image drifted from the golden reference by more than {MAX_CHANNEL_DIFF} per channel"
    );
}

/// Two colored point lights plus a dim directional fill, visibly
/// distinguishable — the concrete proof the multi-light shader loop
/// actually runs through the real binary, not just `extract_scene`'s unit
/// coverage (see Phase 4 / ADR-0019).
#[test]
fn render_of_a_multi_light_scene_matches_golden_image_within_tolerance() {
    let out = scratch_png();
    Command::cargo_bin("engine")
        .unwrap()
        .env_remove("DISPLAY")
        .args([
            "render",
            MULTI_LIGHT_SCENE,
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
    let golden = image::open(MULTI_LIGHT_GOLDEN).unwrap().into_rgba8();
    assert!(
        images_match_within_tolerance(&fresh, &golden),
        "rendered image drifted from the golden reference by more than {MAX_CHANNEL_DIFF} per channel"
    );
}

/// A cube casting a visible shadow onto the ground plane below it — the
/// concrete proof the shadow-map pass actually darkens occluded fragments
/// through the real binary, not just `extract_scene`'s unit coverage (see
/// Phase 5 / ADR-0019).
#[test]
fn render_of_a_shadow_casting_scene_matches_golden_image_within_tolerance() {
    let out = scratch_png();
    Command::cargo_bin("engine")
        .unwrap()
        .env_remove("DISPLAY")
        .args([
            "render",
            SHADOW_SCENE,
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
    let golden = image::open(SHADOW_GOLDEN).unwrap().into_rgba8();
    assert!(
        images_match_within_tolerance(&fresh, &golden),
        "rendered image drifted from the golden reference by more than {MAX_CHANNEL_DIFF} per channel"
    );
}

#[test]
fn rendering_a_scene_with_more_than_one_shadow_caster_is_a_structured_error() {
    let out = scratch_png();
    Command::cargo_bin("engine")
        .unwrap()
        .env_remove("DISPLAY")
        .args([
            "render",
            "tests/fixtures/scenes/render_multiple_shadow_casters.toml",
            "--to",
            out.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("RENDER_MULTIPLE_SHADOW_CASTERS"));
}

#[test]
fn rendering_a_scene_with_a_point_light_shadow_caster_is_a_structured_error() {
    let out = scratch_png();
    Command::cargo_bin("engine")
        .unwrap()
        .env_remove("DISPLAY")
        .args([
            "render",
            "tests/fixtures/scenes/render_point_light_shadow_caster.toml",
            "--to",
            out.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "RENDER_UNSUPPORTED_SHADOW_CASTER",
        ));
}

#[test]
fn rendering_a_scene_with_more_than_max_lights_is_a_structured_error() {
    let out = scratch_png();
    Command::cargo_bin("engine")
        .unwrap()
        .env_remove("DISPLAY")
        .args([
            "render",
            "tests/fixtures/scenes/render_too_many_lights.toml",
            "--to",
            out.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("RENDER_TOO_MANY_LIGHTS"));
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
