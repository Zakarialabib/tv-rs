//! Error types for the ATR v2 client.

use thiserror::Error;

/// Result alias for the ATR v2 client.
pub type Result<T> = std::result::Result<T, AtrError>;

/// Errors the ATR v2 client can produce.
#[derive(Debug, Error)]
pub enum AtrError {
    /// TLS handshake or I/O failure on the pairing or control port.
    #[error("tls/io: {0}")]
    Tls(#[from] std::io::Error),

    /// Rustls-specific TLS error.
    #[error("rustls: {0}")]
    Rustls(#[from] rustls::Error),

    /// The pairing handshake was rejected (bad secret, bad status).
    #[error("pairing rejected: {0}")]
    PairingRejected(String),

    /// The device sent a status other than STATUS_OK.
    #[error("device returned status {status}")]
    DeviceStatus {
        /// The raw `OuterMessage::Status` value the device returned.
        status: i32,
    },

    /// The protobuf frame could not be decoded.
    #[error("protobuf decode: {0}")]
    Decode(#[from] prost::DecodeError),

    /// The protobuf frame could not be encoded.
    #[error("protobuf encode: {0}")]
    Encode(#[from] prost::EncodeError),

    /// A varint length prefix could not be decoded (stream desync).
    #[error("bad varint length prefix")]
    BadVarint,

    /// The device closed the connection unexpectedly.
    #[error("connection closed by device")]
    ConnectionClosed,

    /// Timeout waiting for a device response.
    #[error("timeout waiting for device response")]
    Timeout,
}
