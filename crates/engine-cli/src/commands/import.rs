use std::path::Path;
use std::process::ExitCode;

use crate::commands::OutputFormat;
use crate::diagnostics::CliError;

pub fn run(input: &Path, assets_dir: &Path, out: Option<&Path>, format: OutputFormat) -> ExitCode {
    let fragment = match crate::import_asset(input, assets_dir) {
        Ok(result) => result.fragment,
        Err(e) => {
            e.print(format.is_json());
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

pub(crate) fn gltf_fragment(imported: &engine_assets::ImportedAsset) -> String {
    let mut fragment = String::new();
    fragment.push_str("[[entity]]\nname = \"imported\"\n\n");
    fragment.push_str("[entity.components.Transform]\nposition = [0.0, 0.0, 0.0]\n\n");
    fragment.push_str(&format!(
        "[entity.components.MeshRef]\nmesh = {{ asset = \"{}\" }}\n",
        imported.mesh_hash
    ));
    if let Some(skin_hash) = &imported.skin_hash {
        fragment.push_str(&format!("skin = \"{skin_hash}\"\n"));
    }
    fragment.push('\n');
    fragment.push_str(&format!(
        "[entity.components.Material]\ncolor = [{:.6}, {:.6}, {:.6}]\n",
        imported.base_color[0], imported.base_color[1], imported.base_color[2]
    ));
    if let Some(texture_hash) = &imported.texture_hash {
        fragment.push_str(&format!("texture = \"{texture_hash}\"\n"));
    }
    fragment.push_str(&format!(
        "roughness = {:.6}\nmetallic = {:.6}\n",
        imported.roughness_factor, imported.metallic_factor
    ));
    if let Some(mr_hash) = &imported.metallic_roughness_texture_hash {
        fragment.push_str(&format!("metallic_roughness_texture = \"{mr_hash}\"\n"));
    }
    if let (Some(skeleton_hash), Some(clip_hash)) = (&imported.skeleton_hash, &imported.clip_hash) {
        fragment.push_str(&format!(
            "\n[entity.components.Animator]\nskeleton = \"{skeleton_hash}\"\nclip = \"{clip_hash}\"\n"
        ));
    }
    fragment
}

pub(crate) fn texture_fragment(hash: &str) -> String {
    format!(
        "# Paste into an existing entity's [entity.components.Material] block:\ntexture = \"{hash}\"\n"
    )
}

pub(crate) fn font_fragment(hash: &str) -> String {
    format!(
        "# Paste into an existing entity's [entity.components.Text] block:\nfont = \"{hash}\"\n"
    )
}

pub(crate) fn audio_fragment(hash: &str) -> String {
    format!(
        "# Paste as a new entity's [entity.components.AudioSource] block, \
         or use \"{hash}\" directly in a scripted engine.play_sound() call:\nclip = \"{hash}\"\n"
    )
}
