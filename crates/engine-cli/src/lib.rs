pub mod commands;
pub mod diagnostics;
pub mod recording;
pub mod registry;
pub mod scenarios;
pub mod watch;

use std::path::PathBuf;

use diagnostics::CliError;
use engine_core::inspect::ComponentDumper;
use engine_core::sim::Sim;
use engine_scene::ComponentRegistry;
use engine_script::{DispatchCtx, Script, ScriptHost};

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

    /// Like `build`, but also scans the built world for `Script` components
    /// (per ADR-0006) and — if any are found — loads a `ScriptHost` with
    /// every distinct referenced `.lua` file. Scenes/scenarios with no
    /// `Script` components pay nothing extra: `host` comes back `None`.
    fn build_with_scripts(
        &self,
        seed: u64,
    ) -> Result<(Sim, Vec<ComponentDumper>, Option<ScriptHost>), CliError> {
        let (sim, dumpers) = self.build(seed)?;

        let mut paths: Vec<String> = sim
            .world
            .query::<&Script>()
            .iter()
            .map(|(_, s)| s.path.clone())
            .collect();
        if paths.is_empty() {
            return Ok((sim, dumpers, None));
        }
        paths.sort();
        paths.dedup();

        let mut host = ScriptHost::new().map_err(|e| CliError::from_script_error(&e))?;
        for path in &paths {
            host.load_file(path.as_ref())
                .map_err(|e| CliError::from_script_error(&e))?;
        }
        Ok((sim, dumpers, Some(host)))
    }
}

/// Advances `sim` by one tick, then — if `host` is present — dispatches
/// every `Script`-tagged entity's function once. The first dispatch error
/// (if any) becomes a hard `CliError`, the same failure posture as any other
/// bad scene: `test`/`run`/`replay` are meant to fail loudly, not limp on
/// with partially-applied script output. `watch` mode (see `watch.rs`)
/// handles the "don't crash on a bad edit" requirement at a different
/// layer, by catching this `Err` per rerun instead of suppressing it here.
fn step_and_dispatch(
    sim: &mut Sim,
    dumpers: &[ComponentDumper],
    host: Option<&mut ScriptHost>,
    components: &ComponentRegistry,
) -> Result<(), CliError> {
    sim.step();
    if let Some(host) = host {
        let errors = host.dispatch(DispatchCtx {
            world: &mut sim.world,
            components,
            dumpers,
            tick: sim.tick,
            dt: sim.dt,
        });
        if let Some((_, e)) = errors.into_iter().next() {
            return Err(CliError::from_script_error(&e));
        }
    }
    Ok(())
}

/// Builds `source` into a live `Sim` without dumping it to JSON — what
/// `render` needs (direct `World` access), unlike every other command,
/// which only ever wants the JSON dump `run_and_dump` produces.
pub fn build_sim(source: impl Into<SimSource>, seed: u64) -> Result<Sim, CliError> {
    source.into().build(seed).map(|(sim, _)| sim)
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
    run_and_dump_with_script_paths(source, seed, ticks).map(|(json, _)| json)
}

/// Like `run_and_dump`, but also returns every distinct `.lua` script path
/// loaded along the way (not the scene path itself — callers already have
/// that). Lets `watch` build its file-watch set without building the `Sim`
/// a second time just to ask what scripts it used.
pub(crate) fn run_and_dump_with_script_paths(
    source: impl Into<SimSource>,
    seed: u64,
    ticks: u64,
) -> Result<(serde_json::Value, Vec<PathBuf>), CliError> {
    let (mut sim, dumpers, mut host) = source.into().build_with_scripts(seed)?;
    let components = registry::components();
    for _ in 0..ticks {
        step_and_dispatch(&mut sim, &dumpers, host.as_mut(), &components)?;
    }
    let script_paths = host
        .as_ref()
        .map(|h| h.loaded_paths().map(PathBuf::from).collect())
        .unwrap_or_default();
    let json = engine_core::inspect::world_to_json(&sim.world, sim.tick, seed, &dumpers);
    Ok((json, script_paths))
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
    let (mut sim, dumpers, mut host) = source.into().build_with_scripts(seed)?;
    let components = registry::components();
    let mut snapshots = Vec::new();
    for t in 1..=ticks {
        step_and_dispatch(&mut sim, &dumpers, host.as_mut(), &components)?;
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
