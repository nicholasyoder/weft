use std::path::Path;
use std::process::ExitCode;

use crate::commands::OutputFormat;
use crate::SimSource;

pub fn run(scene: &Path, seed: u64, ticks: u64, format: OutputFormat) -> ExitCode {
    if ticks == 0 {
        crate::diagnostics::CliError::invalid_ticks(ticks).print(format.is_json());
        return ExitCode::FAILURE;
    }

    match crate::run_and_dump(SimSource::Scene(scene.to_path_buf()), seed, ticks) {
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
