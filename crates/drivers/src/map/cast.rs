//! Google Cast error mapping to unified `TvError`.

use tv_rs_core::capability::Capability;
use tv_rs_core::error::{AuthError, ProtocolError, TransportError, TvError};
use crate::cast::error::CastError;

/// Maps a raw `CastError` into the shared `TvError` routing taxonomy.
#[must_use]
pub fn to_tv_error(error: &CastError, _capability: Capability) -> TvError {
    match error {
        CastError::Http(err) => TvError::Transport(TransportError::Unreachable(err.to_string())),
        CastError::Io(err) => TvError::Transport(TransportError::from_io(err)),
        CastError::Protobuf(err) => TvError::Protocol(ProtocolError::Malformed(err.to_string())),
        CastError::Auth(msg) => TvError::Auth(AuthError::NotPermitted {
            remedy: msg.clone(),
        }),
        CastError::Json(err) => TvError::Protocol(ProtocolError::Malformed(err.to_string())),
    }
}
