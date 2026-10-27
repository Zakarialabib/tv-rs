//! Errors returned by the IR control and scanner crate.

use tv_rs_core::TvError;

/// Why an IR operation failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IrError {
    /// Emitter error (e.g. LIRC device failure or network bridge error).
    #[error("ir emitter error: {0}")]
    Emitter(String),

    /// Protocol error or invalid frame encoding.
    #[error("ir protocol error: {0}")]
    Protocol(String),

    /// Universal scanner failed to detect a working profile.
    #[error("ir scanner failed to detect brand: {0}")]
    ScanFailed(String),

    /// Profile loading or parsing failed.
    #[error("ir profile error: {0}")]
    Profile(String),

    /// Feedback loop error during scanning.
    #[error("feedback error: {0}")]
    Feedback(String),
}

impl From<IrError> for TvError {
    fn from(err: IrError) -> Self {
        match err {
            IrError::Emitter(msg) => {
                TvError::Transport(tv_rs_core::TransportError::Unreachable(msg))
            }
            IrError::Protocol(msg) => TvError::Protocol(tv_rs_core::ProtocolError::Malformed(msg)),
            IrError::ScanFailed(msg) => {
                TvError::Protocol(tv_rs_core::ProtocolError::Unexpected(msg))
            }
            IrError::Profile(msg) => TvError::Protocol(tv_rs_core::ProtocolError::Malformed(msg)),
            IrError::Feedback(msg) => {
                TvError::Transport(tv_rs_core::TransportError::Unreachable(msg))
            }
        }
    }
}
