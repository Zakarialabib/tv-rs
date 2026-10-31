//! Error types for the Google Cast client.

/// Errors returned by the Google Cast client.
#[derive(Debug, thiserror::Error)]
pub enum CastError {
    /// Transport-level failure raised by reqwest.
    #[error("http transport error: {0}")]
    Http(#[from] reqwest::Error),

    /// Standard IO error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Protobuf decoding error.
    #[error("Protobuf error: {0}")]
    Protobuf(#[from] prost::DecodeError),

    /// TLS or EKE authentication failed.
    #[error("Authentication failed: {0}")]
    Auth(String),

    /// JSON serialization/deserialization error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Convenience alias for `Result` values produced by this crate.
pub type Result<T> = std::result::Result<T, CastError>;
