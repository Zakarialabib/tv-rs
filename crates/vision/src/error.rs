//! Error types for vision processing, RTSP camera streams, and AI inference backends.

use thiserror::Error;

/// Errors produced by the `tv-rs-vision` crate.
#[derive(Debug, Error)]
pub enum VisionError {
    /// RTSP stream error or connection failure.
    #[error("RTSP stream error: {0}")]
    RtspStream(String),

    /// Frame decoding or format error.
    #[error("Frame decode error: {0}")]
    FrameDecode(String),

    /// Model inference error (ONNX Runtime, OpenCV, or model execution).
    #[error("Inference error: {0}")]
    Inference(String),

    /// Invalid configuration or parameters.
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    /// IO or system error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Result type for vision operations.
pub type Result<T> = std::result::Result<T, VisionError>;
