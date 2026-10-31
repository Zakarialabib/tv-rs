//! Google Cast sender implementation for smart TVs and Chromecasts.
//!
//! Control channel uses TLS on port 8009 with length-prefixed CASTV2
//! protobuf messages (the device's self-signed certificate is accepted).
//! Fallback to DIAL over HTTP port 8008 for launch/stop.

#![forbid(unsafe_code)]

pub mod error;
pub mod proto;

pub mod client;
pub mod dial;
pub mod payload;
pub mod transport;
