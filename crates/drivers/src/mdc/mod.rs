//! Samsung Multiple Display Control (MDC) commercial driver.

pub mod client;
pub mod driver;
pub mod error;
pub mod handle;
pub mod payload;
pub mod protocol;
mod transport;
pub mod types;

pub use client::{MdcClient, MdcOptions};
pub use driver::MdcDriver;
pub use error::MdcError;
pub use handle::MdcDevice;
pub use payload::{ErrorStatus, Status};
pub use protocol::{
    commands, Frame, Response, ACK, HEADER, MDC_TCP_PORT, NAK, RESPONSE_COMMAND, TERMINATOR,
};
pub use types::{DisplayId, InputSource, MdcOrientation, MdcPower};
