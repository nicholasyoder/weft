pub mod commands;
pub mod diagnostics;
pub mod recording;
pub mod registry;
pub mod scenarios;

use std::path::PathBuf;

use diagnostics::CliError;
use engine_core::inspect::ComponentDumper;
use engine_core::sim::Sim;

/// Where a `Sim` comes from: a hardcoded Rust scenario (Phase 0) or a
/// text scene file (Phase 1). Every CLI command that runs a simulation
/// takes one of these so `test`/`inspect`/`run`/`replay` share one
/// build-and-error path regardless of source.
#[derive(Clone)]
pub enum SimSource {
    Scenario(String),
    Scene(PathBuf),
}

impl From<&str> for SimSource {
    fn from(name: &str) -> Self {
        SimSource::Scenario(name.to_string())
    }
}

impl From<String> for SimSource {
    fn from(name: String) -> Self {
        SimSource::Scenario(name)
    }
}

impl SimSource {
    /// Human-readable label for messages and diagnostics — a scenario name
    /// or a scene file path.
    pub fn label(&self) -> String {
        match self {
            SimSource::Scenario(name) => name.clone(),
            SimSource::Scene(path) => path.display().to_string(),
        }
    }

    fn build(&self, seed: u64) -> Result<(Sim, Vec<ComponentDumper>), CliError> {
        match self {
            SimSource::Scenario(name) => {
                let s = scenarios::find(name).ok_or_else(|| CliError::unknown_scenario(name))?;
                Ok(((s.build)(seed), s.dumpers.to_vec()))
            }
            SimSource::Scene(path) => {
                engine_scene::load(path, seed, &registry::components(), &registry::systems())
                    .map_err(|e| CliError::from_scene_error(path, &e))
            }
        }
    }
}

/// Builds `source`, runs it for `ticks` ticks, and returns the final
/// world-state JSON dump. The one place scenario/scene build + run + inspect
/// are joined, so every caller (CLI commands and tests alike) shares this
/// path.
pub fn run_and_dump(
    source: impl Into<SimSource>,
    seed: u64,
    ticks: u64,
) -> Result<serde_json::Value, CliError> {
    let (mut sim, dumpers) = source.into().build(seed)?;
    sim.run(ticks);
    Ok(engine_core::inspect::world_to_json(
        &sim.world, sim.tick, seed, &dumpers,
    ))
}

/// Runs `source` for `ticks` ticks, dumping a snapshot every `dump_every`
/// ticks (plus a final snapshot at the end if it doesn't already land on a
/// boundary). Returns one JSON value per snapshot, in tick order.
pub fn run_and_dump_snapshots(
    source: impl Into<SimSource>,
    seed: u64,
    ticks: u64,
    dump_every: u64,
) -> Result<Vec<serde_json::Value>, CliError> {
    let (mut sim, dumpers) = source.into().build(seed)?;
    let mut snapshots = Vec::new();
    for t in 1..=ticks {
        sim.step();
        if t % dump_every == 0 || t == ticks {
            snapshots.push(engine_core::inspect::world_to_json(
                &sim.world, sim.tick, seed, &dumpers,
            ));
        }
    }
    Ok(snapshots)
}

pub struct DeterminismFailure {
    pub source: String,
    pub json_a: serde_json::Value,
    pub json_b: serde_json::Value,
}

/// Runs `source` twice with the same seed and asserts the resulting
/// world-state JSON is byte-identical. This single function backs both the
/// `engine test` CLI subcommand and the `cargo test` regression suite —
/// there is exactly one implementation of "is this deterministic."
pub fn verify_scenario_determinism(
    source: impl Into<SimSource>,
    seed: u64,
    ticks: u64,
) -> Result<serde_json::Value, DeterminismResult> {
    let source = source.into();
    let json_a = run_and_dump(source.clone(), seed, ticks).map_err(DeterminismResult::Error)?;
    let json_b = run_and_dump(source.clone(), seed, ticks).map_err(DeterminismResult::Error)?;
    if json_a == json_b {
        Ok(json_a)
    } else {
        Err(DeterminismResult::Mismatch(DeterminismFailure {
            source: source.label(),
            json_a,
            json_b,
        }))
    }
}

pub enum DeterminismResult {
    Error(CliError),
    Mismatch(DeterminismFailure),
}
