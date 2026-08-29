use std::path::Path;
use std::process::ExitCode;

use engine_assets::AssetStore;

use crate::commands::OutputFormat;
use crate::diagnostics::CliError;

pub fn run(input: &Path, assets_dir: &Path, out: Option<&Path>, format: OutputFormat) -> ExitCode {
    let store = AssetStore::new(assets_dir);
    let extension = input
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    let fragment = match extension.as_str() {
        "gltf" | "glb" => match engine_assets::import_gltf(input, &store) {
            Ok(imported) => gltf_fragment(&imported),
            Err(e) => {
                CliError::from_asset_error(&e).print(format.is_json());
                return ExitCode::FAILURE;
            }
        },
        "png" | "jpg" | "jpeg" | "bmp" | "gif" | "tga" | "webp" => {
            match engine_assets::import_texture(input, &store) {
                Ok(hash) => texture_fragment(&hash),
                Err(e) => {
                    CliError::from_asset_error(&e).print(format.is_json());
                    return ExitCode::FAILURE;
                }
            }
        }
        other => {
            CliError::unsupported_import_extension(input, other).print(format.is_json());
            return ExitCode::FAILURE;
        }
    };

    if let Some(out) = out {
        if let Err(source) = std::fs::write(out, &fragment) {
            CliError::import_write_failed(out, &source).print(format.is_json());
            return ExitCode::FAILURE;
        }
    }

    if format.is_json() {
        println!(
            "{}",
            serde_json::json!({
                "status": "ok",
                "input": input.display().to_string(),
                "assets_dir": assets_dir.display().to_string(),
                "out": out.map(|p| p.display().to_string()),
                "fragment": fragment,
            })
        );
    } else if let Some(out) = out {
        println!("imported '{}' -> '{}'", input.display(), out.display());
    } else {
        print!("{fragment}");
    }
    ExitCode::SUCCESS
}

fn gltf_fragment(imported: &engine_assets::ImportedAsset) -> String {
    let mut fragment = String::new();
    fragment.push_str("[[entity]]\nname = \"imported\"\n\n");
    fragment.push_str("[entity.components.Transform]\nposition = [0.0, 0.0, 0.0]\n\n");
    fragment.push_str(&format!(
        "[entity.components.MeshRef]\nmesh = {{ asset = \"{}\" }}\n\n",
        imported.mesh_hash
    ));
    fragment.push_str(&format!(
        "[entity.components.Material]\ncolor = [{:.6}, {:.6}, {:.6}]\n",
        imported.base_color[0], imported.base_color[1], imported.base_color[2]
    ));
    if let Some(texture_hash) = &imported.texture_hash {
        fragment.push_str(&format!("texture = \"{texture_hash}\"\n"));
    }
    fragment
}

fn texture_fragment(hash: &str) -> String {
    format!(
        "# Paste into an existing entity's [entity.components.Material] block:\ntexture = \"{hash}\"\n"
    )
}
