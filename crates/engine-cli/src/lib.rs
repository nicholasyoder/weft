pub mod commands;
pub mod diagnostics;
pub mod recording;
pub mod scenarios;

use diagnostics::CliError;

/// Builds `scenario`, runs it for `ticks` ticks, and returns the final
/// world-state JSON dump. The one place scenario-run + inspect are joined,
/// so every caller (CLI commands and tests alike) shares this path.
pub fn run_and_dump(scenario: &str, seed: u64, ticks: u64) -> Result<serde_json::Value, CliError> {
    let s = scenarios::find(scenario).ok_or_else(|| CliError::unknown_scenario(scenario))?;
    let mut sim = (s.build)(seed);
    sim.run(ticks);
    Ok(engine_core::inspect::world_to_json(
        &sim.world, sim.tick, seed, s.dumpers,
    ))
}

/// Runs `scenario` for `ticks` ticks, dumping a snapshot every `dump_every`
/// ticks (plus a final snapshot at the end if it doesn't already land on a
/// boundary). Returns one JSON value per snapshot, in tick order.
pub fn run_and_dump_snapshots(
    scenario: &str,
    seed: u64,
    ticks: u64,
    dump_every: u64,
) -> Result<Vec<serde_json::Value>, CliError> {
    let s = scenarios::find(scenario).ok_or_else(|| CliError::unknown_scenario(scenario))?;
    let mut sim = (s.build)(seed);
    let mut snapshots = Vec::new();
    for t in 1..=ticks {
        sim.step();
        if t % dump_every == 0 || t == ticks {
            snapshots.push(engine_core::inspect::world_to_json(
                &sim.world, sim.tick, seed, s.dumpers,
            ));
        }
    }
    Ok(snapshots)
}

pub struct DeterminismFailure {
    pub scenario: String,
    pub json_a: serde_json::Value,
    pub json_b: serde_json::Value,
}

/// Runs `scenario` twice with the same seed and asserts the resulting
/// world-state JSON is byte-identical. This single function backs both the
/// `engine test` CLI subcommand and the `cargo test` regression suite —
/// there is exactly one implementation of "is this scenario deterministic."
pub fn verify_scenario_determinism(
    scenario: &str,
    seed: u64,
    ticks: u64,
) -> Result<serde_json::Value, DeterminismResult> {
    let json_a = run_and_dump(scenario, seed, ticks).map_err(DeterminismResult::Error)?;
    let json_b = run_and_dump(scenario, seed, ticks).map_err(DeterminismResult::Error)?;
    if json_a == json_b {
        Ok(json_a)
    } else {
        Err(DeterminismResult::Mismatch(DeterminismFailure {
            scenario: scenario.to_string(),
            json_a,
            json_b,
        }))
    }
}

pub enum DeterminismResult {
    Error(CliError),
    Mismatch(DeterminismFailure),
}
