use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use assert_cmd::Command;

const BOX_TEXTURED: &str = "tests/fixtures/gltf/box_textured.gltf";

fn scratch_dir() -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("engine-cli-import-test-{}-{n}", std::process::id()))
}

fn count_files_recursive(dir: &std::path::Path) -> usize {
    if !dir.exists() {
        return 0;
    }
    let mut count = 0;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        for entry in std::fs::read_dir(&current).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                stack.push(entry.path());
            } else {
                count += 1;
            }
        }
    }
    count
}

#[test]
fn import_emits_a_pasteable_scene_fragment() {
    let dir = scratch_dir();
    let assets_dir = dir.join("assets");
    let out = dir.join("fragment.toml");

    Command::cargo_bin("engine")
        .unwrap()
        .args([
            "import",
            BOX_TEXTURED,
            "--assets-dir",
            assets_dir.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
        ])
        .assert()
        .success();

    let fragment = std::fs::read_to_string(&out).unwrap();
    assert!(fragment.contains("[entity.components.MeshRef]"));
    assert!(fragment.contains("mesh = { asset = \""));
    assert!(fragment.contains("[entity.components.Material]"));
    assert!(fragment.contains("texture = \""));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn reimporting_the_same_file_produces_no_spurious_diff_or_churn() {
    let dir = scratch_dir();
    let assets_dir = dir.join("assets");
    let out = dir.join("fragment.toml");

    let run_import = || {
        Command::cargo_bin("engine")
            .unwrap()
            .args([
                "import",
                BOX_TEXTURED,
                "--assets-dir",
                assets_dir.to_str().unwrap(),
                "--out",
                out.to_str().unwrap(),
            ])
            .assert()
            .success();
    };

    run_import();
    let first_fragment = std::fs::read_to_string(&out).unwrap();
    let first_file_count = count_files_recursive(&assets_dir);

    run_import();
    let second_fragment = std::fs::read_to_string(&out).unwrap();
    let second_file_count = count_files_recursive(&assets_dir);

    assert_eq!(
        first_fragment, second_fragment,
        "re-importing must produce byte-identical output"
    );
    assert_eq!(
        first_file_count, second_file_count,
        "re-importing must not add or change any files in the asset store"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn importing_an_unsupported_extension_is_a_structured_error() {
    let dir = scratch_dir();
    Command::cargo_bin("engine")
        .unwrap()
        .args([
            "import",
            "Cargo.toml",
            "--assets-dir",
            dir.join("assets").to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("IMPORT_UNSUPPORTED_EXTENSION"));
}
