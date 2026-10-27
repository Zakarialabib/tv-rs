//! # tv-rs-vision (The Eyes)
//!
//! Computer vision & edge AI event engine for the `tv-rs` ecosystem.
//!
//! Handles RTSP camera streams, runs edge-optimized AI models (YOLO object detection,
//! MediaPipe pose/gesture tracking), and outputs structured JSON events.
//!
//! ## Example
//!
//! ```rust,no_run
//! use tv_rs_vision::{
//!     VisionConfig, VisionPipeline, MockEngine, Zone, VisionEvent,
//! };
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let mut config = VisionConfig::default();
//!     config.zones.push(Zone::new("entrance", 0.0, 0.0, 0.5, 1.0));
//!
//!     let engine = Box::new(MockEngine::new());
//!     let mut pipeline = VisionPipeline::new(config, engine);
//!
//!     pipeline.connect().await?;
//!     let events = pipeline.step().await?;
//!
//!     for event in events {
//!         let json_str = serde_json::to_string(&event)?;
//!         println!("Event JSON: {}", json_str);
//!     }
//!
//!     Ok(())
//! }
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod analytics;
pub mod engine;
pub mod error;
pub mod pipeline;
pub mod stream;
pub mod types;

pub use analytics::EventProcessor;
pub use engine::{InferenceResult, MediaPipeTracker, MockEngine, VisionEngine, YoloDetector};
pub use error::{Result, VisionError};
pub use pipeline::VisionPipeline;
pub use stream::{FrameFormat, RtspStreamReader, VideoFrame};
pub use types::{
    BoundingBox, Detection, GestureType, InferenceBackend, Pose, PoseLandmark, RtspStreamConfig,
    VisionConfig, VisionEvent, Zone,
};
