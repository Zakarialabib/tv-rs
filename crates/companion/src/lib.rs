//! `tv-rs-companion` — Mobile companion core for digital signage displays.
//!
//! # Example
//!
//! ```rust
//! use tv_rs_companion::CompanionDevice;
//!
//! let companion = CompanionDevice::new("host-1", "192.168.1.100", 8080);
//! assert_eq!(companion.host_address(), "192.168.1.100");
//! ```

#![forbid(unsafe_code)]
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

/// Remote key definitions and input dispatch.
pub mod remote;

/// Power and input selection abstractions.
pub mod control;

/// QR and SAS pairing handshake ceremonies.
pub mod ceremony;

/// Discovery with multicast-lock / entitlement gating.
pub mod discovery;

/// Protocol interaction with tv-rs-hostd daemon.
pub mod hostd;

/// Errors produced during companion interactions.
pub mod error;

/// UniFFI export definitions and bridge types.
pub mod uniffi_types;

/// Device handle implementation for the companion.
pub mod handle;

pub use error::{CompanionError, Result};
pub use handle::CompanionDevice;
