//! Roku error mapping to unified `TvError`.

use crate::roku::error::RokuError;
use tv_rs_core::capability::Capability;
use tv_rs_core::error::{AuthError, ProtocolError, TransportError, TvError};

/// Maps a Roku error to a unified `TvError`.
#[must_use]
pub fn to_tv_error(err: &RokuError, capability: Capability) -> TvError {
    match err {
        RokuError::Unsupported(_) => TvError::Unsupported(capability),
        RokuError::NotPermitted { remedy } => TvError::Auth(AuthError::NotPermitted {
            remedy: remedy.clone(),
        }),
        RokuError::NotFound(detail) => TvError::Protocol(ProtocolError::Unexpected(detail.clone())),
        RokuError::Xml(detail) => TvError::Protocol(ProtocolError::Malformed(detail.clone())),
        RokuError::Http(_) => TvError::Transport(TransportError::Unreachable(err.to_string())),
    }
}
