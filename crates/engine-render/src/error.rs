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
    #[error("failed to create a window surface: {0}")]
    SurfaceCreateFailed(String),
    #[error("failed to acquire the next surface frame: {0}")]
    SurfaceAcquireFailed(String),
    #[error("failed to write PNG to '{path}': {source}")]
    EncodeFailed {
        path: String,
        #[source]
        source: image::ImageError,
    },
    #[error(transparent)]
    AssetLoadFailed(#[from] engine_assets::AssetError),
    #[error("failed to parse font: {0}")]
    FontParseFailed(String),
    #[error(
        "mesh/skin vertex count mismatch: mesh has {mesh} vertices, skin has {skin} — they must come from the same glTF primitive import"
    )]
    SkinVertexCountMismatch { mesh: usize, skin: usize },
}

impl RenderError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NoCamera => "RENDER_NO_CAMERA",
            Self::MultipleCameras(_) => "RENDER_MULTIPLE_CAMERAS",
            Self::AdapterRequestFailed(_) => "RENDER_ADAPTER_ERROR",
            Self::DeviceRequestFailed(_) => "RENDER_DEVICE_ERROR",
            Self::ReadbackFailed(_) => "RENDER_READBACK_ERROR",
            Self::SurfaceCreateFailed(_) => "RENDER_SURFACE_CREATE_ERROR",
            Self::SurfaceAcquireFailed(_) => "RENDER_SURFACE_ACQUIRE_ERROR",
            Self::EncodeFailed { .. } => "RENDER_ENCODE_ERROR",
            Self::AssetLoadFailed(_) => "RENDER_ASSET_ERROR",
            Self::FontParseFailed(_) => "RENDER_FONT_PARSE_ERROR",
            Self::SkinVertexCountMismatch { .. } => "RENDER_SKIN_VERTEX_MISMATCH",
        }
    }
}
