//! Error types for `tv-rs-power`.

use thiserror::Error;

/// Convenience alias for results returned by this crate.
pub type Result<T> = std::result::Result<T, PowerError>;

/// Errors produced by Wake-on-LAN transmission and power-state probing.
#[derive(Debug, Error)]
pub enum PowerError {
    /// The requested capability is not implemented because it is blocked on a
    /// future reverse-engineering phase.
    #[error(transparent)]
    Unsupported(#[from] tv_rs_transport::Unsupported),
    /// An underlying I/O operation failed (socket bind/send, TCP connect,
    /// broadcast configuration, ...).
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    /// The supplied MAC address string was malformed.
    #[error("invalid MAC address: {0}")]
    InvalidMacAddress(String),
    /// The supplied host:port address was malformed.
    #[error("invalid broadcast/service address: {0}")]
    InvalidBroadcastAddress(String),
}
