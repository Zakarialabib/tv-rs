//! Error types for the Roku ECP client.

/// Errors returned by the Roku External Control Protocol (ECP) client.
#[derive(Debug, thiserror::Error)]
pub enum RokuError {
    /// The requested capability is not available in this implementation phase.
    ///
    /// Roku ECP itself is fully public; this variant exists for behaviour that
    /// still needs reverse-engineering work (see the playbook's Phase 2/3 REA
    /// gaps) before it can be implemented for real.
    #[error(transparent)]
    Unsupported(#[from] tv_rs_transport::Unsupported),

    /// Transport-level failure raised by [`reqwest`].
    #[error("http transport error: {0}")]
    Http(#[from] reqwest::Error),

    /// The device returned a body that could not be parsed as the expected XML.
    #[error("xml parse error: {0}")]
    Xml(String),

    /// A requested resource (for example an application by name) was not found.
    #[error("not found: {0}")]
    NotFound(String),
}

/// Convenience alias for `Result` values produced by this crate.
pub type Result<T> = std::result::Result<T, RokuError>;
