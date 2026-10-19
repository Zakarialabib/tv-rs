//! Error types returned by the `tv-rs-tizen` crate.

use thiserror::Error;

/// Error type for every fallible operation in this crate.
#[derive(Debug, Error)]
pub enum TizenError {
    /// A capability that is intentionally deferred until the reverse-engineering
    /// blockers in `docs/01-playbook.md` §10 are resolved.
    ///
    /// This replaces the old `unimplemented!()` stubs: the public API must never
    /// panic, so blocked feature gaps surface as a recoverable error instead.
    #[error(transparent)]
    Unsupported(#[from] tv_rs_transport::Unsupported),

    /// WebSocket transport error.
    #[error("websocket error: {0}")]
    Ws(#[from] tokio_tungstenite::tungstenite::Error),

    /// HTTP error from the DIAL client.
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),

    /// JSON (de)serialization error.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    /// URL construction or parse error.
    #[error("url error: {0}")]
    Url(#[from] url::ParseError),

    /// TLS setup failure, e.g. building the `native-tls` connector.
    #[error("tls error: {0}")]
    Tls(String),

    /// The TV rejected the pairing request (`ms.channel.unauthorized`).
    #[error("unauthorized: the TV rejected the on-screen pairing request")]
    Unauthorized,

    /// The frame budget was exhausted before a usable reply arrived.
    #[error("timeout: frame budget exhausted before a reply arrived")]
    Timeout,

    /// The peer sent a frame that violates the expected protocol shape.
    #[error("protocol error: {0}")]
    Protocol(String),
}

/// Convenience `Result` alias defaulting to [`TizenError`].
pub type Result<T> = std::result::Result<T, TizenError>;
