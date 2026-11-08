//! Tizen error mapping to unified `TvError`.

use tv_rs_core::capability::Capability;
use tv_rs_core::error::{AuthError, ProtocolError, TransportError, TvError};
use crate::tizen::error::TizenError;

/// Maps a Tizen error to a unified `TvError`.
#[must_use]
pub fn to_tv_error(err: &TizenError, capability: Capability) -> TvError {
    match err {
        TizenError::Unsupported(_) => TvError::Unsupported(capability),
        TizenError::Tls(detail) => TvError::Transport(TransportError::Tls(detail.clone())),
        TizenError::Unauthorized => TvError::Auth(AuthError::PairingRequired),
        TizenError::InvalidKey(_) => TvError::Auth(AuthError::TokenExpired),
        TizenError::InvalidHost(detail) => {
            TvError::Transport(TransportError::Unreachable(detail.clone()))
        }
        TizenError::Timeout => TvError::Transport(TransportError::Timeout),
        TizenError::Ws(_) | TizenError::Http(_) => {
            TvError::Transport(TransportError::Unreachable(err.to_string()))
        }
        TizenError::Io(io) => TvError::Transport(TransportError::from_io(io)),
        TizenError::Json(_) | TizenError::Xml(_) | TizenError::Url(_) => {
            TvError::Protocol(ProtocolError::Malformed(err.to_string()))
        }
        TizenError::Protocol(detail) => TvError::Protocol(ProtocolError::Malformed(detail.clone())),
    }
}
