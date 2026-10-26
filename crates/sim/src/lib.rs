//! `tv-rs-sim` — Hardware-free protocol simulators for smart TVs and commercial panels.
//!
//! Provides mock protocol servers for testing TV controllers without hardware:
//! - [`mdc::MockMdcServer`]: Samsung MDC binary TCP protocol server on port 1515.
//! - [`webos::MockWebosServer`]: LG webOS SSAP WebSocket protocol server.
//! - [`tizen::MockTizenServer`]: Samsung Tizen remote channel WebSocket protocol server.

#![forbid(unsafe_code)]

pub mod mdc;
pub mod tizen;
pub mod webos;
