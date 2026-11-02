//! Hardware mock servers for tv-rs (MDC, SICP and webOS).

#![forbid(unsafe_code)]

pub mod mdc_server;
pub mod sicp_server;
pub mod webos_server;

pub use mdc_server::MockMdcServer;
pub use sicp_server::MockSicpServer;
pub use webos_server::{MockWebOsServer, PromptAcceptance};
