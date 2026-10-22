//! Samsung Multiple Display Control (MDC) over TCP.
//!
//! MDC is the control protocol Samsung commercial signage exposes on RJ45 port
//! [`MDC_TCP_PORT`] (1515) and on RS-232. This crate implements the TCP subset:
//!
//! * frame encoding and checksum validation;
//! * display-id addressing, including a broadcast address for video walls;
//! * one shared socket for every display in a wall;
//! * power, input, volume, mute, brightness and aggregated status;
//! * the LFD subset a signage panel needs: panel key lock (`0x5F`), content
//!   orientation (`0xC8`/`0x82`), signal state and panel health (`0x0D`), and
//!   panel on-time (`0x83`, behind the `supporting` feature).
//!
//! It deliberately does not guess: a `NAK` is returned with the command and error
//! code, and the socket is kept open because the wire is still in sync. A timeout
//! or malformed reply drops the socket so the next command reconnects. Screen
//! capture has no MDC command, so the adapter never advertises it.
//!
//! # Example
//!
//! ```no_run
//! use tv_rs_mdc::{DisplayId, MdcClient};
//!
//! # async fn run() -> Result<(), tv_rs_mdc::MdcError> {
//! let wall = MdcClient::new("192.168.1.20");
//! let left = wall.display(DisplayId::new(1)?);
//! let right = wall.display(DisplayId::new(2)?);
//!
//! left.power_on().await?;
//! right.set_input(tv_rs_mdc::InputSource::Hdmi1).await?;
//! right.set_brightness(60).await?;
//!
//! let status = left.status().await?;
//! println!("left is {:?} with volume {:?}", status.power, status.volume);
//! # Ok(())
//! # }
//! ```
//!
//! The optional trailing carriage return is off by default. Enable it with
//! [`MdcOptions::with_line_terminator`] for TV firmware that requires it.

#![forbid(unsafe_code)]

pub mod client;
pub mod error;
pub mod handle;
pub mod payload;
pub mod protocol;
pub mod types;

pub use client::{MdcClient, MdcOptions};
pub use error::MdcError;
pub use handle::MdcDevice;
pub use payload::{ErrorStatus, Status};
pub use protocol::{
    commands, Frame, Response, ACK, HEADER, MDC_TCP_PORT, NAK, RESPONSE_COMMAND, TERMINATOR,
};
pub use types::{DisplayId, InputSource, MdcOrientation, MdcPower};
