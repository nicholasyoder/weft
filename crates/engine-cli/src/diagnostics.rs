use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct CliError {
    pub code: &'static str,
    pub message: String,
    #[serde(default)]
    pub context: serde_json::Value,
}

impl CliError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            context: serde_json::Value::Null,
        }
    }

    pub fn with_context(mut self, context: serde_json::Value) -> Self {
        self.context = context;
        self
    }

    pub fn unknown_scenario(name: &str) -> Self {
        Self::new(
            "SCENARIO_NOT_FOUND",
            format!(
                "no scenario named '{name}' (available: {})",
                crate::scenarios::names().join(", ")
            ),
        )
        .with_context(
            serde_json::json!({ "requested": name, "available": crate::scenarios::names() }),
        )
    }

    pub fn invalid_ticks(ticks: u64) -> Self {
        Self::new(
            "INVALID_TICKS",
            format!("ticks must be greater than 0, got {ticks}"),
        )
    }

    pub fn recording_read_failed(path: &str, source: &std::io::Error) -> Self {
        Self::new(
            "RECORDING_READ_ERROR",
            format!("failed to read recording file '{path}': {source}"),
        )
        .with_context(serde_json::json!({ "path": path }))
    }

    pub fn recording_parse_failed(path: &str, source: &serde_json::Error) -> Self {
        Self::new(
            "RECORDING_PARSE_ERROR",
            format!("failed to parse recording file '{path}': {source}"),
        )
        .with_context(serde_json::json!({ "path": path }))
    }

    pub fn print(&self, json: bool) {
        if json {
            eprintln!("{}", serde_json::json!({ "error": self }));
        } else {
            eprintln!("error[{}]: {}", self.code, self.message);
        }
    }
}
