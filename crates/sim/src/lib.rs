//! Hardware mock servers for tv-rs (MDC, SICP, webOS, Tizen and DLNA/GENA).

#![forbid(unsafe_code)]

pub mod gena_renderer;
pub mod mdc_server;
pub mod sicp_server;
pub mod tizen_server;
pub mod webos_server;

pub use gena_renderer::{MockGenaRenderer, MockNotifySink};
pub use mdc_server::MockMdcServer;
pub use sicp_server::MockSicpServer;
pub use tizen_server::{MockTizenServer, MOCK_TLS_PORT};
pub use webos_server::{MockWebOsServer, PromptAcceptance};
