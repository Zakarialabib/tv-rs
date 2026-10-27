//! `tv-rs-spatial-audio` — Spatial audio management for Bluetooth A2DP sinks and local PulseAudio/PipeWire streams.
//!
//! Controls directional sound domes, ceiling speaker arrays, Bluetooth audio sinks, and
//! local audio streams based on 3D listener/customer coordinates in front of displays.

#![forbid(unsafe_code)]

pub mod backend;
pub mod error;
pub mod router;
pub mod sink;
pub mod types;

pub use backend::{AudioBackend, MockAudioBackend, PulsePipeWireBackend};
pub use error::SpatialAudioError;
pub use router::SpatialAudioRouter;
pub use sink::{AudioSink, AudioSinkKind, AudioStream, SinkState};
pub use types::{ListenerPosition, Position3D, RoutingDecision, Zone};
