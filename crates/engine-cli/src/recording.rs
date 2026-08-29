use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::SimSource;

#[derive(Debug, Serialize, Deserialize)]
pub struct Recording {
    pub version: u32,
    #[serde(default)]
    pub scenario: Option<String>,
    #[serde(default)]
    pub scene: Option<PathBuf>,
    pub seed: u64,
    pub ticks: u64,
    #[serde(default)]
    pub dump_every: Option<u64>,
    /// Reserved for a future real input stream. Unused in Phase 0/1 — there
    /// is no input system yet, so building a recorder for it would be
    /// speculative work ahead of need.
    #[serde(default)]
    pub inputs: Vec<serde_json::Value>,
}

impl Recording {
    pub fn load(path: &std::path::Path) -> Result<Self, crate::diagnostics::CliError> {
        let path_str = path.display().to_string();
        let text = std::fs::read_to_string(path)
            .map_err(|e| crate::diagnostics::CliError::recording_read_failed(&path_str, &e))?;
        let recording: Self = serde_json::from_str(&text)
            .map_err(|e| crate::diagnostics::CliError::recording_parse_failed(&path_str, &e))?;
        if recording.scenario.is_some() == recording.scene.is_some() {
            return Err(crate::diagnostics::CliError::recording_invalid_source(
                &path_str,
            ));
        }
        Ok(recording)
    }

    /// The `Recording`'s source, resolved to a `SimSource`. `load` already
    /// validated that exactly one of `scenario`/`scene` is set.
    pub fn source(&self) -> SimSource {
        match (&self.scenario, &self.scene) {
            (Some(name), None) => SimSource::Scenario(name.clone()),
            (None, Some(path)) => SimSource::Scene(path.clone()),
            _ => unreachable!("Recording::load validates exactly one source is set"),
        }
    }
}
