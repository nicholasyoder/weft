use std::path::Path;
use std::process::ExitCode;

use crate::commands::OutputFormat;
use crate::SimSource;

#[allow(clippy::too_many_arguments)]
pub fn run(
    scene: &Path,
    seed: u64,
    ticks: u64,
    watch: bool,
    assets_dir: &Path,
    format: OutputFormat,
) -> ExitCode {
    if ticks == 0 {
        crate::diagnostics::CliError::invalid_ticks(ticks).print(format.is_json());
        return ExitCode::FAILURE;
    }

    if watch {
        return crate::watch::run(scene, seed, ticks, assets_dir, format);
    }

    match crate::run_and_dump_with_assets_dir(
        SimSource::Scene(scene.to_path_buf()),
        seed,
        ticks,
        assets_dir,
    ) {
        Ok(world) => {
            if format.is_json() {
                println!(
                    "{}",
                    serde_json::json!({
                        "status": "ok",
                        "scene": scene.display().to_string(),
                        "seed": seed,
                        "ticks": ticks,
                        "world": world,
                    })
                );
            } else {
                println!("ran '{}' for {ticks} ticks at seed {seed}", scene.display());
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            e.print(format.is_json());
            ExitCode::FAILURE
        }
    }
}
