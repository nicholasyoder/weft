use std::process::ExitCode;

use crate::commands::OutputFormat;
use crate::recording::Recording;

pub fn run(path: &std::path::Path, assets_dir: &std::path::Path, format: OutputFormat) -> ExitCode {
    let recording = match Recording::load(path) {
        Ok(r) => r,
        Err(e) => {
            e.print(format.is_json());
            return ExitCode::FAILURE;
        }
    };

    if recording.ticks == 0 {
        crate::diagnostics::CliError::invalid_ticks(recording.ticks).print(format.is_json());
        return ExitCode::FAILURE;
    }

    let source = recording.source();
    let result = match recording.dump_every {
        Some(every) if every > 0 => crate::run_and_dump_snapshots_with_assets_dir(
            source,
            recording.seed,
            recording.ticks,
            every,
            assets_dir,
        )
        .map(|snapshots| serde_json::json!({ "snapshots": snapshots })),
        _ => {
            crate::run_and_dump_with_assets_dir(source, recording.seed, recording.ticks, assets_dir)
                .map(|world| serde_json::json!({ "world": world }))
        }
    };

    match result {
        Ok(json) => {
            println!("{}", serde_json::to_string_pretty(&json).unwrap());
            ExitCode::SUCCESS
        }
        Err(e) => {
            e.print(format.is_json());
            ExitCode::FAILURE
        }
    }
}
