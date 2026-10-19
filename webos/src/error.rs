//! Error type for LG webOS SSAP operations.

use thiserror::Error;

/// Convenience alias for results returned by this crate.
pub type Result<T> = std::result::Result<T, WebosError>;

/// Every failure mode produced by `tv-rs-webos`.
#[derive(Debug, Error)]
pub enum WebosError {
    /// The requested capability is blocked pending a later reverse-engineering phase.
    ///
    /// The [`tv_rs_transport::Unsupported`] payload names the phase that unblocks
    /// it and the gap itself.
    #[error(transparent)]
    Unsupported(#[from] tv_rs_transport::Unsupported),

    /// The underlying WebSocket transport failed.
    #[error("websocket error: {0}")]
    Ws(#[from] tokio_tungstenite::tungstenite::Error),

    /// A payload could not be encoded or decoded as JSON.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    /// Local I/O (including `client-key` persistence) failed.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// The endpoint URL could not be parsed.
    #[error("url error: {0}")]
    Url(#[from] url::ParseError),

    /// The TV answered with an SSAP-level `error` frame.
    #[error("ssap error {code}: {message}")]
    Ssap {
        /// Numeric SSAP error code.
        code: i64,
        /// Human-readable SSAP error message.
        message: String,
    },

    /// The registration handshake was rejected by the TV.
    #[error("registration failed: {0}")]
    RegistrationFailed(String),

    /// The peer sent a frame this client could not interpret for the current step.
    #[error("protocol error: {0}")]
    Protocol(String),

    /// No matching response arrived within the session frame budget.
    #[error("timed out waiting for a matching SSAP response")]
    Timeout,
}
