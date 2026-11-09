//! SICP error mapping to unified `TvError`.

use crate::sicp::error::SicpError;
use tv_rs_core::error::{ProtocolError, TransportError, TvError};

/// Maps a raw SICP error into the shared `TvError` routing taxonomy.
#[must_use]
pub fn to_tv_error(error: &SicpError) -> TvError {
    match error {
        SicpError::Io(err) => TvError::Transport(TransportError::from_io(err)),
        SicpError::Timeout => TvError::Transport(TransportError::Timeout),
        SicpError::Protocol(message) => {
            TvError::Protocol(ProtocolError::Malformed(message.clone()))
        }
        SicpError::Nak { command, code } => TvError::Protocol(ProtocolError::Unexpected(format!(
            "display returned NAK 0x{code:02X} for command 0x{command:02X}"
        ))),
        other @ (SicpError::FrameTooLong(_)
        | SicpError::InvalidDisplayId(_)
        | SicpError::InvalidValue { .. }
        | SicpError::BroadcastRequiresSendOnly) => {
            TvError::Protocol(ProtocolError::Malformed(other.to_string()))
        }
    }
}
