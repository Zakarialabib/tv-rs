//! `tv-rs-provision` — First-boot provisioning via BLE Improv Wi-Fi and QR pairing.
//!
//! # Example
//!
//! ```rust
//! use tv_rs_provision::improv::{ImprovCommand, ImprovState};
//!
//! let cmd = ImprovCommand::Identify;
//! assert_eq!(cmd.command_type(), 0x03);
//! ```

#![forbid(unsafe_code)]
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

/// Improv BLE protocol implementation.
pub mod improv;

/// QR pairing handover structures and validation.
pub mod qr;

/// Multicast lock and network permissions gating.
pub mod multicast;

/// Errors produced during provisioning.
pub mod error;

pub use error::{ProvisionError, Result};
