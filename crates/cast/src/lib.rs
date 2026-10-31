//! Google Cast sender implementation for smart TVs and Chromecasts.
//!
//! Control channel uses TLS on port 8009 and EKE2 handshake.
//! Fallback to DIAL over HTTP port 8008 for launch/stop.

#![forbid(unsafe_code)]

pub mod error;
pub mod proto;

pub mod client;
pub mod dial;
pub mod payload;
pub mod transport;
