use std::path::Path;
use std::process::ExitCode;

use crate::commands::OutputFormat;
use crate::recording::Recording;
use crate::SimSource;

pub enum Source {
    Inline {
        source: SimSource,
        seed: u64,
        ticks: u64,
    },
    Recording {
        path: std::path::PathBuf,
    },
}

pub fn run(source: Source, assets_dir: &Path, format: OutputFormat) -> ExitCode {
    let (sim_source, seed, ticks) = match source {
        Source::Inline {
            source,
            seed,
            ticks,
        } => (source, seed, ticks),
        Source::Recording { path } => match Recording::load(&path) {
            Ok(r) => (r.source(), r.seed, r.ticks),
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

    match crate::run_and_dump_with_assets_dir(sim_source, seed, ticks, assets_dir) {
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
