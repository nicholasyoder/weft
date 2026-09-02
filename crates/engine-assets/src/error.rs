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
    #[error("failed to encode skin data: {0}")]
    SkinEncodeFailed(String),
    #[error("failed to decode skin data: {0}")]
    SkinDecodeFailed(String),
    #[error("failed to encode skeleton data: {0}")]
    SkeletonEncodeFailed(String),
    #[error("failed to decode skeleton data: {0}")]
    SkeletonDecodeFailed(String),
    #[error("failed to encode animation data: {0}")]
    AnimationEncodeFailed(String),
    #[error("failed to decode animation data: {0}")]
    AnimationDecodeFailed(String),
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
            Self::SkinEncodeFailed(_) | Self::SkinDecodeFailed(_) => "ASSET_ENCODE_ERROR",
            Self::SkeletonEncodeFailed(_) | Self::SkeletonDecodeFailed(_) => "ASSET_ENCODE_ERROR",
            Self::AnimationEncodeFailed(_) | Self::AnimationDecodeFailed(_) => "ASSET_ENCODE_ERROR",
        }
    }
}
