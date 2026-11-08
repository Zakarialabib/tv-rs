//! CEC error types.

use thiserror::Error;

/// Why a CEC command could not be sent.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CecError {
    /// No adapter or client binary is present.
    #[error("cec unavailable: {0}")]
    Unavailable(String),
    /// The adapter answered, and refused the command.
    #[error("cec rejected the command: {0}")]
    Rejected(String),
    /// The process or pipe failed.
    #[error("cec transport failed: {0}")]
    Io(String),
}
