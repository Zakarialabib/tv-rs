//! Error types for `tv-rs-discovery`.

/// Convenience result alias using [`DiscoveryError`].
pub type Result<T> = std::result::Result<T, DiscoveryError>;

/// Everything that can go wrong during discovery or descriptor parsing.
#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    /// The requested capability is not implemented yet.
    #[error(transparent)]
    Unsupported(#[from] tv_rs_transport::Unsupported),
    /// An I/O error occurred while talking to the network.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// An HTTP error occurred while fetching a device descriptor.
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    /// The XML document could not be parsed.
    #[error("xml parse error: {0}")]
    Xml(String),
    /// The payload was syntactically valid but did not contain the expected data.
    #[error("malformed payload: {0}")]
    Malformed(String),
}
