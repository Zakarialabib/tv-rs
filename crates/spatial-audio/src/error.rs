//! Errors and error conversions for spatial audio operations.

use thiserror::Error;
use tv_rs_core::{Capability, TvError};

/// Typed errors produced by spatial audio routing and sink control.
#[derive(Debug, Error)]
pub enum SpatialAudioError {
    /// Audio sink was not found in registered sinks list.
    #[error("audio sink not found: {0}")]
    SinkNotFound(String),

    /// Audio stream was not found.
    #[error("audio stream not found: {0}")]
    StreamNotFound(String),

    /// Spatial audio backend daemon or interface is unavailable.
    #[error("spatial audio backend unavailable: {0}")]
    BackendUnavailable(String),

    /// Connection to audio sink failed.
    #[error("failed to connect to sink {sink_id}: {reason}")]
    ConnectionFailed {
        /// Target sink ID.
        sink_id: String,
        /// Reason for failure.
        reason: String,
    },

    /// Bluetooth stack or A2DP pairing error.
    #[error("bluetooth A2DP error: {0}")]
    BluetoothError(String),

    /// Invalid 3D spatial coordinates or bounds provided.
    #[error("invalid 3D spatial coordinates: {0}")]
    InvalidCoordinates(String),

    /// System command execution failed.
    #[error("audio system command failed: {0}")]
    CommandFailed(String),
}

impl From<SpatialAudioError> for TvError {
    fn from(err: SpatialAudioError) -> Self {
        match err {
            SpatialAudioError::SinkNotFound(msg) | SpatialAudioError::StreamNotFound(msg) => {
                TvError::protocol(format!("spatial audio element missing: {msg}"))
            }
            SpatialAudioError::BackendUnavailable(reason) => {
                TvError::Transport(tv_rs_core::TransportError::Unreachable(format!(
                    "audio daemon unreachable: {reason}"
                )))
            }
            SpatialAudioError::ConnectionFailed { sink_id, reason } => {
                TvError::Transport(tv_rs_core::TransportError::Unreachable(format!(
                    "audio sink {sink_id} connection failed: {reason}"
                )))
            }
            SpatialAudioError::BluetoothError(msg) => {
                TvError::Auth(tv_rs_core::AuthError::NotPermitted {
                    remedy: format!("bluetooth A2DP error: {msg}"),
                })
            }
            SpatialAudioError::InvalidCoordinates(msg) => {
                TvError::protocol(format!("invalid coordinates: {msg}"))
            }
            SpatialAudioError::CommandFailed(_) => TvError::Unsupported(Capability::SpatialAudio),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spatial_audio_error_converts_to_tv_error() {
        let err = SpatialAudioError::SinkNotFound("speaker_1".into());
        let tv_err: TvError = err.into();
        assert!(matches!(tv_err, TvError::Protocol(_)));
    }
}
