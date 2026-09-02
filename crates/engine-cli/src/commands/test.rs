use std::path::Path;
use std::process::ExitCode;

use crate::commands::OutputFormat;
use crate::{verify_scenario_determinism_with_assets_dir, DeterminismResult, SimSource};

pub fn run(
    source: SimSource,
    seed: u64,
    ticks: u64,
    assets_dir: &Path,
    format: OutputFormat,
) -> ExitCode {
    if ticks == 0 {
        crate::diagnostics::CliError::invalid_ticks(ticks).print(format.is_json());
        return ExitCode::FAILURE;
    }

    let label = source.label();
    match verify_scenario_determinism_with_assets_dir(source, seed, ticks, assets_dir) {
        Ok(json) => {
            if format.is_json() {
                println!(
                    "{}",
                    serde_json::json!({
                        "status": "pass",
                        "source": label,
                        "seed": seed,
                        "ticks": ticks,
                        "world": json,
                    })
                );
            } else {
                println!("PASS: '{label}' is deterministic over {ticks} ticks at seed {seed}");
            }
            ExitCode::SUCCESS
        }
        Err(DeterminismResult::Error(e)) => {
            e.print(format.is_json());
            ExitCode::FAILURE
        }
        Err(DeterminismResult::Mismatch(fail)) => {
            if format.is_json() {
                println!(
                    "{}",
                    serde_json::json!({
                        "status": "fail",
                        "source": fail.source,
                        "reason": "nondeterministic",
                        "run_a": fail.json_a,
                        "run_b": fail.json_b,
                    })
                );
            } else {
                println!(
                    "FAIL: '{}' is NOT deterministic — two runs with the same seed produced different world state",
                    fail.source
                );
            }
            ExitCode::FAILURE
        }
    }
}
