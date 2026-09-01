#[derive(Debug, thiserror::Error)]
pub enum ScriptError {
    #[error("failed to read script file '{path}': {source}")]
    ReadFailed {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to load script '{path}': {source}")]
    LoadFailed {
        path: String,
        #[source]
        source: mlua::Error,
    },
    #[error("script '{path}' has no function named '{function}'")]
    UnknownFunction { path: String, function: String },
    #[error("script '{path}' function '{function}' failed for entity '{entity}': {source}")]
    RuntimeFailed {
        path: String,
        function: String,
        entity: String,
        #[source]
        source: Box<mlua::Error>,
    },
    #[error(
        "script '{path}' function '{function}' returned a value entity '{entity}' couldn't be updated with: {source}"
    )]
    ResultDecodeFailed {
        path: String,
        function: String,
        entity: String,
        #[source]
        source: Box<mlua::Error>,
    },
    #[error(
        "script '{path}' function '{function}' returned an unknown component '{component}' for entity '{entity}'"
    )]
    UnknownComponent {
        path: String,
        function: String,
        entity: String,
        component: String,
    },
    #[error(
        "script '{path}' function '{function}' returned component '{component}' for entity '{entity}' that failed to deserialize: {source}"
    )]
    ComponentDeserializeFailed {
        path: String,
        function: String,
        entity: String,
        component: String,
        #[source]
        source: serde_json::Error,
    },
}

impl ScriptError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::ReadFailed { .. } => "SCRIPT_READ_ERROR",
            Self::LoadFailed { .. } => "SCRIPT_LOAD_ERROR",
            Self::UnknownFunction { .. } => "SCRIPT_UNKNOWN_FUNCTION",
            Self::RuntimeFailed { .. } => "SCRIPT_RUNTIME_ERROR",
            Self::ResultDecodeFailed { .. } => "SCRIPT_RESULT_ERROR",
            Self::UnknownComponent { .. } => "SCRIPT_UNKNOWN_COMPONENT",
            Self::ComponentDeserializeFailed { .. } => "SCRIPT_COMPONENT_ERROR",
        }
    }
}
