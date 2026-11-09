//! CEC error mapping to unified `TvError`.

use crate::cec::error::CecError;
use tv_rs_core::error::{ProtocolError, TransportError, TvError};

/// Maps a `CecError` into the shared `TvError` routing taxonomy.
#[must_use]
pub fn to_tv_error(error: &CecError) -> TvError {
    match error {
        CecError::Unavailable(msg) => TvError::Transport(TransportError::Unreachable(msg.clone())),
        CecError::Rejected(msg) => TvError::Protocol(ProtocolError::Unexpected(msg.clone())),
        CecError::Io(msg) => TvError::Transport(TransportError::Unreachable(msg.clone())),
    }
}
