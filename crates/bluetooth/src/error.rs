//! Error types for Bluetooth HID operation.

use thiserror::Error;
use tv_rs_core::{AuthError, StateError, TransportError, TvError};

/// Reasons a Bluetooth HID operation or transport failed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BluetoothError {
    /// Bluetooth adapter or transport is unavailable or turned off.
    #[error("bluetooth adapter unavailable: {0}")]
    AdapterUnavailable(String),

    /// Failed to pair or connect to target Bluetooth device.
    #[error("bluetooth connection failed: {0}")]
    ConnectionFailed(String),

    /// The device rejected or failed to acknowledge a HID report transmission.
    #[error("bluetooth hid report transmission failed: {0}")]
    TransmitFailed(String),

    /// The requested key or character cannot be mapped to USB HID keycodes.
    #[error("unsupported keycode or character: {0}")]
    UnsupportedKey(String),

    /// Device is disconnected.
    #[error("bluetooth device disconnected")]
    Disconnected,

    /// Invalid Bluetooth device MAC address.
    #[error("invalid bluetooth address format: {0}")]
    InvalidAddress(String),
}

impl From<BluetoothError> for TvError {
    fn from(err: BluetoothError) -> Self {
        match err {
            BluetoothError::AdapterUnavailable(msg) => {
                TvError::Transport(TransportError::Unreachable(msg))
            }
            BluetoothError::ConnectionFailed(_) => TvError::Auth(AuthError::PairingRequired),
            BluetoothError::TransmitFailed(msg) => {
                TvError::Transport(TransportError::Unreachable(msg))
            }
            BluetoothError::UnsupportedKey(_) => TvError::Unsupported(tv_rs_core::Capability::Key),
            BluetoothError::Disconnected => TvError::State(StateError::NotReady),
            BluetoothError::InvalidAddress(msg) => TvError::protocol(msg),
        }
    }
}
