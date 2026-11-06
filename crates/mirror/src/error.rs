//! Error types for screen mirroring session and packet handling.

use thiserror::Error;

/// Mirror error taxonomy.
#[derive(Debug, Error)]
pub enum MirrorError {
    /// Packet size exceeds the maximum allowed 8 MiB limit.
    #[error("packet payload exceeds maximum size of {limit} bytes (got {size})")]
    PacketTooLarge {
        /// Actual size of the packet.
        size: usize,
        /// Maximum allowed limit (8 MiB).
        limit: usize,
    },

    /// Packet data is shorter than minimum required header bytes.
    #[error("packet buffer too short (expected at least {expected} bytes, got {actual})")]
    BufferTooShort {
        /// Expected minimum bytes.
        expected: usize,
        /// Actual bytes available.
        actual: usize,
    },

    /// Unknown packet kind discriminator byte.
    #[error("unknown packet kind: {0}")]
    UnknownPacketKind(u8),

    /// Missing session when a non-zero session is strictly required (e.g. resize).
    #[error("operation requires a non-zero app window session")]
    NonZeroSessionRequired,

    /// Invalid configuration payload or parameter set.
    #[error("invalid configuration: {0}")]
    InvalidConfiguration(String),

    /// Serialization or deserialization error.
    #[error("serialization error: {0}")]
    Serialization(String),
}

/// A specialized `Result` alias for mirror operations.
pub type Result<T> = std::result::Result<T, MirrorError>;
