//! Philips Serial Information Control Protocol (SICP) over TCP and RS-232 serial.

#![forbid(unsafe_code)]

pub mod client;
pub mod error;
pub mod handle;
pub mod payload;
pub mod protocol;
mod transport;
pub mod types;

pub use client::{SicpClient, SicpOptions};
pub use error::SicpError;
pub use handle::SicpDevice;
pub use payload::Status;
pub use protocol::{commands, control_types, Frame, Response, SICP_TCP_PORT};
pub use types::{DisplayId, SicpInput, SicpPower};
