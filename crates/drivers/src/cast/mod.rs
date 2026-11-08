//! Google Cast sender implementation for smart TVs and Chromecasts.
//!
//! Control channel uses TLS on port 8009 and EKE2 handshake.
//! Fallback to DIAL over HTTP port 8008 for launch/stop.

pub mod error;
pub mod proto;

pub mod client;
pub mod dial;
pub mod driver;
pub mod handle;
pub mod payload;
pub mod transport;

pub use client::CastClient;
pub use driver::CastDriver;
pub use error::{CastError, Result};
pub use handle::CastDeviceHandle;
pub use payload::MediaStatusDto;
