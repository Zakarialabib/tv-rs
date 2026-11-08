//! Philips Serial Information Control Protocol (SICP) driver.

pub mod client;
pub mod driver;
pub mod error;
pub mod handle;
pub mod payload;
pub mod protocol;
mod transport;
pub mod types;

pub use client::{SicpClient, SicpOptions};
pub use driver::SicpDriver;
pub use error::SicpError;
pub use handle::SicpDevice;
pub use payload::Status;
pub use protocol::{commands, control_types, Frame, Response, SICP_TCP_PORT};
pub use types::{DisplayId, SicpInput, SicpPower};
