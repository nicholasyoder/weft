use std::path::Path;
use std::process::ExitCode;

use crate::commands::OutputFormat;

#[allow(clippy::too_many_arguments)]
pub fn run(
    scene: &Path,
    to: &Path,
    assets_dir: &Path,
    seed: u64,
    ticks: u64,
    width: u32,
    height: u32,
    format: OutputFormat,
) -> ExitCode {
    if ticks == 0 {
        crate::diagnostics::CliError::invalid_ticks(ticks).print(format.is_json());
        return ExitCode::FAILURE;
    }

    match crate::render_scene(scene, seed, ticks, width, height, assets_dir, to) {
        Ok(()) => {
            if format.is_json() {
                println!(
                    "{}",
                    serde_json::json!({
                        "status": "ok",
                        "scene": scene.display().to_string(),
                        "to": to.display().to_string(),
                        "width": width,
                        "height": height,
                    })
                );
            } else {
                println!(
                    "rendered '{}' ({width}x{height}) to '{}'",
                    scene.display(),
                    to.display()
                );
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            e.print(format.is_json());
            ExitCode::FAILURE
        }
    }
}
