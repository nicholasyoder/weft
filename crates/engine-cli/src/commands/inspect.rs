use std::process::ExitCode;

use crate::commands::OutputFormat;
use crate::recording::Recording;

pub enum Source {
    Inline {
        scenario: String,
        seed: u64,
        ticks: u64,
    },
    Recording {
        path: std::path::PathBuf,
    },
}

pub fn run(source: Source, format: OutputFormat) -> ExitCode {
    let (scenario, seed, ticks) = match source {
        Source::Inline {
            scenario,
            seed,
            ticks,
        } => (scenario, seed, ticks),
        Source::Recording { path } => match Recording::load(&path) {
            Ok(r) => (r.scenario, r.seed, r.ticks),
            Err(e) => {
                e.print(format.is_json());
                return ExitCode::FAILURE;
            }
        },
    };

    if ticks == 0 {
        crate::diagnostics::CliError::invalid_ticks(ticks).print(format.is_json());
        return ExitCode::FAILURE;
    }

    match crate::run_and_dump(&scenario, seed, ticks) {
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
