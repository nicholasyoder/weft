#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("failed to write mixdown WAV file '{path}': {source}")]
    MixWriteFailed {
        path: String,
        #[source]
        source: hound::Error,
    },
}

impl AudioError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::MixWriteFailed { .. } => "AUDIO_MIX_WRITE_ERROR",
        }
    }
}
