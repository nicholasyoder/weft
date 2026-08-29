use std::process::ExitCode;

use crate::commands::OutputFormat;
use crate::recording::Recording;

pub fn run(path: &std::path::Path, format: OutputFormat) -> ExitCode {
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

    let result = match recording.dump_every {
        Some(every) if every > 0 => crate::run_and_dump_snapshots(
            &recording.scenario,
            recording.seed,
            recording.ticks,
            every,
        )
        .map(|snapshots| serde_json::json!({ "snapshots": snapshots })),
        _ => crate::run_and_dump(&recording.scenario, recording.seed, recording.ticks)
            .map(|world| serde_json::json!({ "world": world })),
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
