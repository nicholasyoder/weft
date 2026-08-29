#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("scene has no Camera entity — add one (Transform + Camera components) to render it")]
    NoCamera,
    #[error("scene has {0} Camera entities — exactly one is required to render")]
    MultipleCameras(usize),
    #[error("failed to find a compatible GPU adapter: {0}")]
    AdapterRequestFailed(String),
    #[error("failed to create a GPU device: {0}")]
    DeviceRequestFailed(String),
    #[error("failed to read back the rendered frame: {0}")]
    ReadbackFailed(String),
    #[error("failed to write PNG to '{path}': {source}")]
    EncodeFailed {
        path: String,
        #[source]
        source: image::ImageError,
    },
}

impl RenderError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NoCamera => "RENDER_NO_CAMERA",
            Self::MultipleCameras(_) => "RENDER_MULTIPLE_CAMERAS",
            Self::AdapterRequestFailed(_) => "RENDER_ADAPTER_ERROR",
            Self::DeviceRequestFailed(_) => "RENDER_DEVICE_ERROR",
            Self::ReadbackFailed(_) => "RENDER_READBACK_ERROR",
            Self::EncodeFailed { .. } => "RENDER_ENCODE_ERROR",
        }
    }
}
