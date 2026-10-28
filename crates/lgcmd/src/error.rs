use thiserror::Error;

/// Error type for LG commercial ASCII serial protocol operations.
#[derive(Debug, Error)]
pub enum LgCmdError {
    /// An I/O error occurred on the connection.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// The TV returned an error response.
    #[error("TV rejected command: {0}")]
    Rejected(String),
    /// The protocol response was malformed.
    #[error("Malformed response: {0}")]
    Malformed(String),
}

/// Result alias for LG command operations.
pub type Result<T> = std::result::Result<T, LgCmdError>;
