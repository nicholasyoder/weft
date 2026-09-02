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
    sample_rate: u32,
    format: OutputFormat,
) -> ExitCode {
    if ticks == 0 {
        crate::diagnostics::CliError::invalid_ticks(ticks).print(format.is_json());
        return ExitCode::FAILURE;
    }

    match crate::mix_scene(scene, seed, ticks, sample_rate, assets_dir, to) {
        Ok(()) => {
            if format.is_json() {
                println!(
                    "{}",
                    serde_json::json!({
                        "status": "ok",
                        "scene": scene.display().to_string(),
                        "to": to.display().to_string(),
                        "sample_rate": sample_rate,
                        "ticks": ticks,
                    })
                );
            } else {
                println!(
                    "mixed '{}' ({ticks} ticks @ {sample_rate}Hz) to '{}'",
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
