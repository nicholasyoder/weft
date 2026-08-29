use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Recording {
    pub version: u32,
    pub scenario: String,
    pub seed: u64,
    pub ticks: u64,
    #[serde(default)]
    pub dump_every: Option<u64>,
    /// Reserved for a future real input stream. Unused in Phase 0 — there is
    /// no input system yet, so building a recorder for it would be
    /// speculative work ahead of need.
    #[serde(default)]
    pub inputs: Vec<serde_json::Value>,
}

impl Recording {
    pub fn load(path: &std::path::Path) -> Result<Self, crate::diagnostics::CliError> {
        let path_str = path.display().to_string();
        let text = std::fs::read_to_string(path)
            .map_err(|e| crate::diagnostics::CliError::recording_read_failed(&path_str, &e))?;
        serde_json::from_str(&text)
            .map_err(|e| crate::diagnostics::CliError::recording_parse_failed(&path_str, &e))
    }
}
