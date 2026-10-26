//! Hardware mock servers for tv-rs (MDC and SICP).

#![forbid(unsafe_code)]

pub mod mdc_server;
pub mod sicp_server;

pub use mdc_server::MockMdcServer;
pub use sicp_server::MockSicpServer;
