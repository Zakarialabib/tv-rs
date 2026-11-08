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
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

#[cfg(test)]
extern crate std;

/// Internal alloc prelude for no_std collections and types.
#[allow(unused_imports)]
pub(crate) mod prelude {
    pub use alloc::borrow::ToOwned;
    pub use alloc::boxed::Box;
    pub use alloc::format;
    pub use alloc::string::{String, ToString};
    pub use alloc::vec;
    pub use alloc::vec::Vec;
}

pub mod capability;
pub mod caps;
pub mod command;
#[cfg(feature = "conformance")]
pub mod conformance;
pub mod descriptor;
pub mod driver;
pub mod error;
pub mod imperative;
pub mod lease;
pub mod observe;
#[cfg(feature = "mock-backend")]
pub mod mock;
pub mod provisioning;
#[cfg(feature = "quirks")]
pub mod quirks;
pub mod state;
pub mod telemetry;
pub mod types;

#[cfg(feature = "serde")]
pub mod serde;

pub use capability::{
    AppLaunch, AppRef, BrightnessControl, Capability, CapabilityRegistry, CapabilitySet, DeviceHandle, Input,
    InputSelect, Key, KeyInput, Orientation, OrientationControl, PanelHealth, PanelHealthProvider,
    PanelHours, PanelLockControl, PowerControl, Probe, Rebootable, ScreenCastProvider,
    SpatialAudioProvider, TextInput, VolumeControl,
};
#[cfg(feature = "supporting")]
pub use capability::{FrameFormat, PanelHoursProvider, ScreenCapturer, ScreenFrame};
pub use caps::{
    CapabilityId, CapabilityMatrix, EndpointCapabilities, HelloUpdate, MulticastLockRequired,
    PermissionStatus, UiTile,
};
pub use command::{Command, CommandAck, CommandId};
#[cfg(feature = "conformance")]
pub use conformance::{ConformanceCase, ConformanceProvider, verify_orphaned_capabilities, Violation};
pub use descriptor::{DeviceDescriptor, DeviceId, DeviceOs, DiscoveredTv, PowerState};
pub use driver::{AnyDriver, BoxedDeviceHandle, Driver, ErasedDriver};
pub use error::{AuthError, ProtocolError, QuirkId, Remedy, StateError, TransportError, TvError};
pub use imperative::ImperativeCommand;
pub use lease::{LeaseMode, MaintenanceLease};
pub use observe::{EventReceiver, ObservationError, ObservationStream, TvEvent};
pub use provisioning::{MdcTopology, NetworkProvisionTarget, ProvisioningBundleV1};
pub use state::{Confidence, ObservedField, ObservedState};
pub use telemetry::{Telemetry, TelemetryProvider};
pub use types::TvVendor;
