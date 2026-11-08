//! MDC error mapping to unified `TvError`.

use tv_rs_core::error::{ProtocolError, TransportError, TvError};
use crate::mdc::error::MdcError;

/// Maps a raw MDC error into the shared `TvError` routing taxonomy.
#[must_use]
pub fn to_tv_error(error: &MdcError) -> TvError {
    match error {
        MdcError::Io(err) => TvError::Transport(TransportError::from_io(err)),
        MdcError::Timeout => TvError::Transport(TransportError::Timeout),
        MdcError::Protocol(message) => TvError::Protocol(ProtocolError::Malformed(message.clone())),
        MdcError::Nak { command, code } => TvError::Protocol(ProtocolError::Unexpected(format!(
            "display returned NAK 0x{code:02X} for command 0x{command:02X}"
        ))),
        other @ (MdcError::FrameTooLong(_)
        | MdcError::InvalidDisplayId(_)
        | MdcError::InvalidValue { .. }
        | MdcError::BroadcastRequiresSendOnly) => {
            TvError::Protocol(ProtocolError::Malformed(other.to_string()))
        }
    }
}
