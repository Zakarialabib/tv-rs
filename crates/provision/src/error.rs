//! Error types for `tv-rs-provision`.

use alloc::string::String;
use thiserror::Error;

/// Crate-level Result type.
pub type Result<T> = core::result::Result<T, ProvisionError>;

/// Everything that can go wrong during first-boot or QR provisioning.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ProvisionError {
    /// Invalid packet framing or checksum failure in Improv BLE frame.
    #[error("Improv BLE frame error: {0}")]
    ImprovFrame(String),

    /// State violation in Improv protocol flow.
    #[error("Improv state transition error: {0}")]
    ImprovState(String),

    /// QR code payload parse or validation failure.
    #[error("QR pairing payload error: {0}")]
    QrPayload(String),

    /// SAS authentication discrepancy.
    #[error("Short Authentication String (SAS) mismatch: {0}")]
    SasMismatch(String),

    /// Multicast lock or network entitlement required.
    #[error("Multicast permission / lock required: {0}")]
    MulticastPermissionRequired(String),

    /// Provisioning target connection failed.
    #[error("Target unreachable: {0}")]
    Unreachable(String),
}
