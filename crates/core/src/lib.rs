//! The device model shared by every `tv-rs` backend.
//!
//! This crate is **vocabulary, not protocol**. It knows a television has
//! capabilities, a state that cannot always be observed, and failures that mean
//! different things — and it knows nothing about ports, handshakes or XML.
//!
//! ```no_run
//! use std::sync::Arc;
//! use tv_rs_core::{CommandId, DeviceHandle, Key, KeyInput};
//!
//! async fn press(handle: &Arc<dyn DeviceHandle>, key: Key) -> Result<(), tv_rs_core::TvError> {
//!     let input = handle
//!         .as_key()
//!         .ok_or(tv_rs_core::TvError::Unsupported(tv_rs_core::Capability::Key))?;
//!     input.key_press(key, CommandId::new()).await
//! }
//! ```
//!
//! # Three rules this crate exists to enforce
//!
//! 1. **No generic leaks.** A fleet holds `Arc<dyn DeviceHandle>`; a caller asks
//!    `as_power()` and treats `None` as "this device cannot do that", which is the
//!    same fact as [`TvError::Unsupported`] but knowable before the attempt.
//! 2. **State carries confidence.** A TV that stopped answering is not off — it is
//!    [`Confidence::Unknown`], and saying so is more useful than a boolean.
//! 3. **Errors route.** [`TvError`] is a decision table: re-pair, reconnect, back
//!    off, or stop. A taxonomy that only describes is a taxonomy nobody uses.
//!
//! # What it deliberately does not do
//!
//! No timers, no retries, no scheduler, no I/O. Backends return futures; the agent
//! crate decides when to call them again. That is why `tokio` is absent from this
//! crate's dependency graph in every configuration.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod capability;
pub mod command;
#[cfg(feature = "conformance")]
pub mod conformance;
pub mod descriptor;
pub mod error;
#[cfg(feature = "mock-backend")]
pub mod mock;
#[cfg(feature = "quirks")]
pub mod quirks;
pub mod state;

pub use capability::{
    AppLaunch, AppRef, Capability, CapabilitySet, DeviceHandle, Input, InputSelect, Key, KeyInput,
    PowerControl, VolumeControl,
};
pub use command::CommandId;
pub use descriptor::{DeviceDescriptor, DeviceId, DeviceOs, PowerState};
pub use error::{AuthError, ProtocolError, QuirkId, StateError, TransportError, TvError};
pub use state::{Confidence, ObservedField, ObservedState};
