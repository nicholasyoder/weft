#[derive(Debug, thiserror::Error)]
pub enum SceneError {
    #[error("failed to read scene file '{path}': {source}")]
    ReadFailed {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse scene file '{path}': {source}")]
    ParseFailed {
        path: String,
        #[source]
        source: toml::de::Error,
    },
    #[error("scene '{path}': entity '{entity}' references unknown component '{component}'")]
    UnknownComponent {
        path: String,
        entity: String,
        component: String,
    },
    #[error(
        "scene '{path}': entity '{entity}' component '{component}' failed to deserialize: {source}"
    )]
    ComponentDeserializeFailed {
        path: String,
        entity: String,
        component: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("scene '{path}': references unknown system '{system}'")]
    UnknownSystem { path: String, system: String },
}

impl SceneError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::ReadFailed { .. } => "SCENE_READ_ERROR",
            Self::ParseFailed { .. } => "SCENE_PARSE_ERROR",
            Self::UnknownComponent { .. } => "SCENE_UNKNOWN_COMPONENT",
            Self::ComponentDeserializeFailed { .. } => "SCENE_COMPONENT_ERROR",
            Self::UnknownSystem { .. } => "SCENE_UNKNOWN_SYSTEM",
        }
    }
}
