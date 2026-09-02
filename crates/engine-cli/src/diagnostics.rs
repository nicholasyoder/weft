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

    pub fn recording_invalid_source(path: &str) -> Self {
        Self::new(
            "RECORDING_INVALID_SOURCE",
            format!("recording file '{path}' must specify exactly one of 'scenario' or 'scene'"),
        )
        .with_context(serde_json::json!({ "path": path }))
    }

    pub fn from_scene_error(path: &std::path::Path, source: &engine_scene::SceneError) -> Self {
        Self::new(source.code(), source.to_string())
            .with_context(serde_json::json!({ "path": path.display().to_string() }))
    }

    pub fn from_render_error(source: &engine_render::RenderError) -> Self {
        Self::new(source.code(), source.to_string())
    }

    pub fn from_asset_error(source: &engine_assets::AssetError) -> Self {
        Self::new(source.code(), source.to_string())
    }

    pub fn from_script_error(source: &engine_script::ScriptError) -> Self {
        Self::new(source.code(), source.to_string())
    }

    pub fn from_audio_error(source: &engine_audio::AudioError) -> Self {
        Self::new(source.code(), source.to_string())
    }

    /// A registered system (`system`) failed mid-tick (`tick`) — see
    /// [ADR-0017](../../../docs/decisions/0017-system-error-channel.md).
    /// `error.code` is the domain-specific code the failing system's own
    /// underlying error type already defines (e.g. `ASSET_NOT_FOUND`,
    /// `AUDIO_CLIP_DECODE_ERROR`), reused verbatim rather than wrapped in a
    /// generic "system failed" code.
    pub fn from_system_error(
        system: &str,
        tick: u64,
        error: &engine_core::scheduler::SystemError,
    ) -> Self {
        Self::new(error.code, error.message.clone())
            .with_context(serde_json::json!({ "system": system, "tick": tick }))
    }

    pub fn unsupported_import_extension(path: &std::path::Path, extension: &str) -> Self {
        Self::new(
            "IMPORT_UNSUPPORTED_EXTENSION",
            format!(
                "don't know how to import '{}': unsupported extension '{extension}' (expected .gltf/.glb, a common image format, .ttf/.otf, or .wav/.ogg)",
                path.display()
            ),
        )
        .with_context(serde_json::json!({ "path": path.display().to_string(), "extension": extension }))
    }

    pub fn import_write_failed(path: &std::path::Path, source: &std::io::Error) -> Self {
        Self::new(
            "IMPORT_WRITE_ERROR",
            format!(
                "failed to write import fragment to '{}': {source}",
                path.display()
            ),
        )
        .with_context(serde_json::json!({ "path": path.display().to_string() }))
    }

    pub fn play_window_init_failed(source: &str) -> Self {
        Self::new(
            "PLAY_WINDOW_INIT_ERROR",
            format!("failed to open a window for 'engine play': {source}"),
        )
    }

    pub fn play_event_loop_failed(source: &str) -> Self {
        Self::new(
            "PLAY_EVENT_LOOP_ERROR",
            format!("the 'engine play' event loop failed: {source}"),
        )
    }

    pub fn print(&self, json: bool) {
        if json {
            eprintln!("{}", serde_json::json!({ "error": self }));
        } else {
            eprintln!("error[{}]: {}", self.code, self.message);
        }
    }
}
