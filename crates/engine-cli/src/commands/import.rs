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
    let multi_part = imported.parts.len() > 1;
    if multi_part {
        fragment.push_str(&format!(
            "# {} entities imported from one file — give them all the same\n\
             # Transform to move/place the group together (this engine has no\n\
             # parent/child transform component yet, see ADR-0020).\n",
            imported.parts.len()
        ));
    }
    for (i, part) in imported.parts.iter().enumerate() {
        let name = if multi_part {
            format!("imported_{i}")
        } else {
            "imported".to_string()
        };
        fragment.push_str(&format!("[[entity]]\nname = \"{name}\"\n\n"));
        fragment.push_str("[entity.components.Transform]\nposition = [0.0, 0.0, 0.0]\n\n");
        fragment.push_str(&format!(
            "[entity.components.MeshRef]\nmesh = {{ asset = \"{}\" }}\n",
            part.mesh_hash
        ));
        if let Some(skin_hash) = &part.skin_hash {
            fragment.push_str(&format!("skin = \"{skin_hash}\"\n"));
        }
        if let Some(tangent_hash) = &part.tangent_hash {
            fragment.push_str(&format!("tangent = \"{tangent_hash}\"\n"));
        }
        fragment.push('\n');
        fragment.push_str(&format!(
            "[entity.components.Material]\ncolor = [{:.6}, {:.6}, {:.6}]\n",
            part.base_color[0], part.base_color[1], part.base_color[2]
        ));
        if let Some(texture_hash) = &part.texture_hash {
            fragment.push_str(&format!("texture = \"{texture_hash}\"\n"));
        }
        fragment.push_str(&format!(
            "roughness = {:.6}\nmetallic = {:.6}\n",
            part.roughness_factor, part.metallic_factor
        ));
        if let Some(mr_hash) = &part.metallic_roughness_texture_hash {
            fragment.push_str(&format!("metallic_roughness_texture = \"{mr_hash}\"\n"));
        }
        if let Some(normal_hash) = &part.normal_texture_hash {
            fragment.push_str(&format!(
                "normal_texture = \"{normal_hash}\"\nnormal_scale = {:.6}\n",
                part.normal_scale
            ));
        }
        if let (true, Some(skeleton_hash), Some(clip_hash)) = (
            part.skin_hash.is_some(),
            &imported.skeleton_hash,
            &imported.clip_hash,
        ) {
            fragment.push_str(&format!(
                "\n[entity.components.Animator]\nskeleton = \"{skeleton_hash}\"\nclip = \"{clip_hash}\"\n"
            ));
        }
        fragment.push('\n');
    }
    // Drop the fragment's trailing blank line — every previous emitter path
    // ended without one, and existing tests/fixtures assert on exact
    // fragment content.
    if fragment.ends_with("\n\n") {
        fragment.pop();
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
