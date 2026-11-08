//! Error types for Philips SICP communication.

use thiserror::Error;
use tv_rs_core::{ProtocolError, TransportError, TvError};

/// An error returned by the Philips SICP client or codec.
#[derive(Debug, Error)]
pub enum SicpError {
    /// An I/O error occurred on the TCP socket or RS-232 line.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Connect or read timeout expired.
    #[error("operation timed out")]
    Timeout,

    /// The display rejected the command (`NAK`).
    #[error("command 0x{command:02X} rejected with error code 0x{code:02X}")]
    Nak {
        /// The command byte echoed in the NAK reply.
        command: u8,
        /// The error code carried in the NAK payload.
        code: u8,
    },

    /// A framing, checksum or protocol violation.
    #[error("protocol error: {0}")]
    Protocol(String),

    /// A payload is too large to fit in the one-byte size field.
    #[error("frame size {0} exceeds maximum byte capacity")]
    FrameTooLong(usize),

    /// A command value is outside the allowed range.
    #[error("{field} {value} is out of bounds (maximum {max})")]
    InvalidValue {
        /// Name of the field.
        field: &'static str,
        /// Value passed.
        value: u16,
        /// Maximum allowed value.
        max: u16,
    },

    /// Broadcast display ID `0x00` / `0xFF` was used with a query or request that expects an answer.
    #[error("broadcast display ID requires send_only")]
    BroadcastRequiresSendOnly,

    /// Display ID is invalid.
    #[error("display id {0} is invalid")]
    InvalidDisplayId(u8),
}

/// Converts a SICP failure into the shared `TvError` routing taxonomy.
impl From<SicpError> for TvError {
    fn from(error: SicpError) -> Self {
        match error {
            SicpError::Io(error) => Self::Transport(TransportError::from_io(&error)),
            SicpError::Timeout => Self::Transport(TransportError::Timeout),
            SicpError::Protocol(message) => Self::Protocol(ProtocolError::Malformed(message)),
            SicpError::Nak { command, code } => Self::Protocol(ProtocolError::Unexpected(format!(
                "display returned NAK 0x{code:02X} for command 0x{command:02X}"
            ))),
            other @ (SicpError::FrameTooLong(_)
            | SicpError::InvalidDisplayId(_)
            | SicpError::InvalidValue { .. }
            | SicpError::BroadcastRequiresSendOnly) => {
                Self::Protocol(ProtocolError::Malformed(other.to_string()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SicpError;

    #[test]
    fn display_formatting() {
        assert_eq!(
            SicpError::Nak {
                command: 0x18,
                code: 0x01
            }
            .to_string(),
            "command 0x18 rejected with error code 0x01"
        );
        assert_eq!(
            SicpError::InvalidValue {
                field: "volume",
                value: 105,
                max: 100
            }
            .to_string(),
            "volume 105 is out of bounds (maximum 100)"
        );
    }
}
