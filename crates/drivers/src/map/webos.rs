//! webOS error mapping to unified `TvError`.

use crate::webos::error::WebosError;
use tv_rs_core::capability::Capability;
use tv_rs_core::error::{AuthError, ProtocolError, TransportError, TvError};

/// Maps a webOS error to a unified `TvError`.
#[must_use]
pub fn to_tv_error(err: &WebosError, capability: Capability) -> TvError {
    match err {
        WebosError::Unsupported(_) => TvError::Unsupported(capability),
        WebosError::RegistrationFailed {
            presented_key: true,
            ..
        } => TvError::Auth(AuthError::TokenExpired),
        WebosError::RegistrationFailed {
            presented_key: false,
            ..
        } => TvError::Auth(AuthError::PairingRequired),
        WebosError::Ssap { code, message } => TvError::Protocol(ProtocolError::Unexpected(
            format!("ssap error {code}: {message}"),
        )),
        WebosError::Timeout => TvError::Transport(TransportError::Timeout),
        WebosError::Ws(_) => TvError::Transport(TransportError::Unreachable(err.to_string())),
        WebosError::Io(io) => TvError::Transport(TransportError::from_io(io)),
        WebosError::Json(_) | WebosError::Url(_) => {
            TvError::Protocol(ProtocolError::Malformed(err.to_string()))
        }
        WebosError::Protocol(detail) => TvError::Protocol(ProtocolError::Malformed(detail.clone())),
    }
}
