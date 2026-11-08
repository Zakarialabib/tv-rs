//! IR error mapping to unified `TvError`.

use tv_rs_core::error::{ProtocolError, TransportError, TvError};
use crate::ir::error::IrError;

/// Maps an IR error to a unified `TvError`.
#[must_use]
pub fn to_tv_error(err: &IrError) -> TvError {
    match err {
        IrError::Emitter(msg) => TvError::Transport(TransportError::Unreachable(msg.clone())),
        IrError::Protocol(msg) => TvError::Protocol(ProtocolError::Malformed(msg.clone())),
        IrError::ScanFailed(msg) => TvError::Protocol(ProtocolError::Unexpected(msg.clone())),
        IrError::Profile(msg) => TvError::Protocol(ProtocolError::Malformed(msg.clone())),
        IrError::Feedback(msg) => TvError::Transport(TransportError::Unreachable(msg.clone())),
    }
}
