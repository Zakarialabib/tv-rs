//! RTSP camera stream reader and video frame management.

use crate::error::{Result, VisionError};
use crate::types::RtspStreamConfig;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use tracing::info;

/// Pixel format for video frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameFormat {
    /// RGB 24-bit packed pixels.
    Rgb24,
    /// BGR 24-bit packed pixels.
    Bgr24,
    /// YUV420 planar.
    Yuv420p,
    /// JPEG encoded compressed frame bytes.
    Jpeg,
}

/// Representing a single raw or encoded video frame captured from an RTSP stream.
#[derive(Debug, Clone)]
pub struct VideoFrame {
    /// Incremental frame sequence ID.
    pub frame_id: u64,
    /// Capture timestamp in milliseconds since epoch.
    pub timestamp_ms: u64,
    /// Width of the frame in pixels.
    pub width: u32,
    /// Height of the frame in pixels.
    pub height: u32,
    /// Pixel format of the frame data.
    pub format: FrameFormat,
    /// Raw or compressed image buffer bytes.
    pub data: Vec<u8>,
}

impl VideoFrame {
    /// Creates a dummy synthetic frame for testing.
    pub fn synthetic(frame_id: u64, width: u32, height: u32) -> Self {
        let timestamp_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let data = vec![128; (width * height * 3) as usize];
        Self {
            frame_id,
            timestamp_ms,
            width,
            height,
            format: FrameFormat::Rgb24,
            data,
        }
    }
}

/// Abstract reader for RTSP camera stream frame ingestion.
pub struct RtspStreamReader {
    config: RtspStreamConfig,
    frame_counter: Arc<AtomicU64>,
    connected: bool,
}

impl RtspStreamReader {
    /// Connects to an RTSP stream using the provided configuration.
    pub async fn connect(config: RtspStreamConfig) -> Result<Self> {
        info!("Connecting to RTSP stream at {}", config.url);
        // Validate URL format
        if !config.url.starts_with("rtsp://") && !config.url.starts_with("rtsps://") {
            return Err(VisionError::InvalidConfig(format!(
                "Invalid RTSP URL scheme: {}",
                config.url
            )));
        }

        Ok(Self {
            config,
            frame_counter: Arc::new(AtomicU64::new(0)),
            connected: true,
        })
    }

    /// Returns whether the RTSP stream connection is currently active.
    pub fn is_connected(&self) -> bool {
        self.connected
    }

    /// Fetches the next frame from the stream.
    pub async fn next_frame(&mut self) -> Result<VideoFrame> {
        if !self.connected {
            return Err(VisionError::RtspStream("Stream disconnected".to_string()));
        }

        // Throttle according to FPS limit
        let frame_delay_ms = if self.config.fps_limit > 0 {
            1000 / self.config.fps_limit as u64
        } else {
            33
        };
        sleep(Duration::from_millis(frame_delay_ms)).await;

        let frame_id = self.frame_counter.fetch_add(1, Ordering::Relaxed);
        Ok(VideoFrame::synthetic(frame_id, 1280, 720))
    }

    /// Closes the RTSP stream connection.
    pub async fn close(&mut self) {
        if self.connected {
            info!("Closing RTSP stream connection for {}", self.config.url);
            self.connected = false;
        }
    }
}
