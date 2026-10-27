//! Universal IR Remote control and scanner crate (`tv-rs-ir`).
//!
//! Provides protocol encodings (NEC, RC5, RC6, SIRC, RCA, Amlogic), hardware/mock
//! emitter abstractions ([`IrEmitter`]), erased [`tv_rs_core::DeviceHandle`] integration,
//! and the [`UniversalScanner`] feedback-loop detection system.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod device;
pub mod emitter;
pub mod error;
pub mod profile;
pub mod protocol;
pub mod scanner;

pub use device::IrDeviceHandle;
pub use emitter::{IrEmitter, LircEmitter, MockIrEmitter, NetworkBridgeEmitter};
pub use error::IrError;
pub use profile::{all_default_profiles, IrProfile};
pub use protocol::{IrProtocol, IrSignal};
pub use scanner::{CecFeedback, IrScanFeedback, MockFeedback, TcpProbeFeedback, UniversalScanner};
