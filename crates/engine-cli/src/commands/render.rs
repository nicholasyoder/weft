use std::path::Path;
use std::process::ExitCode;

use crate::commands::OutputFormat;
use crate::SimSource;

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

    let mut sim = match crate::build_sim(SimSource::Scene(scene.to_path_buf()), seed) {
        Ok(sim) => sim,
        Err(e) => {
            e.print(format.is_json());
            return ExitCode::FAILURE;
        }
    };
    sim.run(ticks);

    match engine_render::render_scene_to_png(&sim.world, width, height, assets_dir, to) {
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
            crate::diagnostics::CliError::from_render_error(&e).print(format.is_json());
            ExitCode::FAILURE
        }
    }
}
