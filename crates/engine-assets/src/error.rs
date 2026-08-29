#[derive(Debug, thiserror::Error)]
pub enum AssetError {
    #[error("failed to read '{path}': {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("no asset found for hash '{0}'")]
    NotFound(String),
    #[error("failed to parse glTF file '{path}': {source}")]
    GltfParseFailed {
        path: String,
        #[source]
        source: gltf::Error,
    },
    #[error("glTF file '{path}' is not supported: {reason}")]
    GltfUnsupported { path: String, reason: String },
    #[error("failed to decode image '{path}': {source}")]
    ImageDecodeFailed {
        path: String,
        #[source]
        source: image::ImageError,
    },
    #[error("failed to encode mesh data: {0}")]
    MeshEncodeFailed(String),
    #[error("failed to decode mesh data: {0}")]
    MeshDecodeFailed(String),
}

impl AssetError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Io { .. } => "ASSET_IO_ERROR",
            Self::NotFound(_) => "ASSET_NOT_FOUND",
            Self::GltfParseFailed { .. } => "ASSET_GLTF_PARSE_ERROR",
            Self::GltfUnsupported { .. } => "ASSET_GLTF_UNSUPPORTED",
            Self::ImageDecodeFailed { .. } => "ASSET_IMAGE_ERROR",
            Self::MeshEncodeFailed(_) | Self::MeshDecodeFailed(_) => "ASSET_ENCODE_ERROR",
        }
    }
}
