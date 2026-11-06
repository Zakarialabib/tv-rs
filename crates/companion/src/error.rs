//! Error types for `tv-rs-companion`.

use alloc::string::{String, ToString};
use thiserror::Error;
use tv_rs_core::TvError;

/// Crate-level Result type.
pub type Result<T> = core::result::Result<T, CompanionError>;

/// Everything that can go wrong in companion operations.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CompanionError {
    /// QR or Short Authentication String verification failed.
    #[error("Authentication ceremony failed: {0}")]
    CeremonyFailed(String),

    /// Multicast lock (Android) or Multicast entitlement (iOS) is missing.
    #[error("Multicast lock or network entitlement missing: {0}")]
    MulticastLocked(String),

    /// Communication with tv-rs-hostd failed.
    #[error("tv-rs-hostd error: {0}")]
    Hostd(String),

    /// Display control command unsupported or rejected.
    #[error("Control rejected: {0}")]
    ControlRejected(String),

    /// Core TvError integration.
    #[error("Core display error: {0}")]
    Core(String),
}

impl From<TvError> for CompanionError {
    fn from(err: TvError) -> Self {
        Self::Core(err.to_string())
    }
}
