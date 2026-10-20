//! Errors returned by the Samsung MDC client.

use tv_rs_core::{ProtocolError, TransportError, TvError};

/// A failure from the MDC socket, frame codec or command layer.
#[derive(Debug, thiserror::Error)]
pub enum MdcError {
    /// The TCP socket could not be opened, read or written.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// Connecting or waiting for an acknowledgement exceeded the configured budget.
    #[error("timed out waiting for the display")]
    Timeout,

    /// The peer sent bytes that are not a valid MDC frame or response.
    #[error("protocol error: {0}")]
    Protocol(String),

    /// The display answered with `NAK`.
    #[error("display rejected command 0x{command:02X} with error 0x{code:02X}")]
    Nak {
        /// The command byte that was rejected.
        command: u8,
        /// The MDC error code returned by the display.
        code: u8,
    },

    /// A data payload exceeded the one-byte MDC length field.
    #[error("frame data is {0} bytes; MDC allows at most 255")]
    FrameTooLong(usize),

    /// A display id outside the addressable range was requested.
    #[error("invalid display id 0x{0:02X}; MDC addresses 0x00-0xFD, with 0xFE as broadcast")]
    InvalidDisplayId(u8),

    /// A value was outside the range accepted by the command.
    #[error("invalid {field} value {value}; the maximum is {max}")]
    InvalidValue {
        /// Field name, for diagnostics.
        field: &'static str,
        /// Rejected value.
        value: u16,
        /// Inclusive maximum.
        max: u16,
    },

    /// Broadcast commands do not receive an acknowledgement; use `send_only`.
    #[error("broadcast commands do not receive a reply; use send_only")]
    BroadcastRequiresSendOnly,
}

/// Converts an MDC failure into the shared routing taxonomy.
///
/// `NAK` is a protocol answer, not a transport failure, so the connection is kept
/// open and the caller sees `ProtocolError::Unexpected`. Socket and timeout
/// failures become `TransportError`, which the agent retries with backoff.
impl From<MdcError> for TvError {
    fn from(error: MdcError) -> Self {
        match error {
            MdcError::Io(error) => Self::Transport(TransportError::Unreachable(error.to_string())),
            MdcError::Timeout => Self::Transport(TransportError::Timeout),
            MdcError::Protocol(message) => Self::Protocol(ProtocolError::Malformed(message)),
            MdcError::Nak { command, code } => Self::Protocol(ProtocolError::Unexpected(format!(
                "display returned NAK 0x{code:02X} for command 0x{command:02X}"
            ))),
            other @ (MdcError::FrameTooLong(_)
            | MdcError::InvalidDisplayId(_)
            | MdcError::InvalidValue { .. }
            | MdcError::BroadcastRequiresSendOnly) => {
                Self::Protocol(ProtocolError::Malformed(other.to_string()))
            }
        }
    }
}
