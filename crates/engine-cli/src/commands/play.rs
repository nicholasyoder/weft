use std::path::Path;
use std::process::ExitCode;

use crate::commands::OutputFormat;

#[allow(clippy::too_many_arguments)]
pub fn run(
    scene: &Path,
    seed: u64,
    assets_dir: &Path,
    width: u32,
    height: u32,
    max_ticks: Option<u64>,
    format: OutputFormat,
) -> ExitCode {
    let result = crate::live::play(
        scene,
        seed,
        assets_dir,
        &crate::registry::components(),
        &crate::registry::systems(),
        width,
        height,
        wgpu::Backends::PRIMARY,
        max_ticks,
    );

    match result {
        Ok(()) => {
            if format.is_json() {
                println!(
                    "{}",
                    serde_json::json!({ "status": "ok", "scene": scene.display().to_string() })
                );
            } else {
                println!("'{}' exited cleanly", scene.display());
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            e.print(format.is_json());
            ExitCode::FAILURE
        }
    }
}
