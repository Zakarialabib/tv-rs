//! `tv-rs-power` — Wake-on-LAN and TV power-state probing.
//!
//! This crate implements the Phase-1 power primitives from the TV-control
//! playbook: broadcasting Wake-on-LAN magic packets (UDP port 9,
//! `255.255.255.255`) and probing whether a TV's main CPU is awake or merely
//! in network-only standby.
//!
//! # Example
//!
//! ```no_run
//! use tv_rs_power::{build_magic_packet, MacAddress, WolSender};
//!
//! # async fn run() -> Result<(), tv_rs_power::PowerError> {
//! let mac: MacAddress = "AA:BB:CC:DD:EE:FF".parse()?;
//! let packet = build_magic_packet(&mac);
//! assert_eq!(packet.as_bytes().len(), 102);
//!
//! let sender = WolSender::new()?;
//! sender.wake(&mac).await?;
//! # Ok(())
//! # }
//! ```

pub mod client;
pub mod error;
pub mod payload;
pub mod protocol;
pub mod types;

pub use client::{build_magic_packet, poll_power_state, wake_on_lan, WolSender};
pub use error::{PowerError, Result};
pub use payload::MagicPacket;
pub use protocol::PowerControl;
pub use types::{
    MacAddress, PowerState, MAC_ADDRESS_LEN, MAGIC_PACKET_LEN, WOL_BROADCAST_ADDR, WOL_UDP_PORT,
};
