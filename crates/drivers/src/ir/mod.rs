//! Universal IR remote control and scanner driver.

pub mod device;
pub mod driver;
pub mod emitter;
pub mod error;
pub mod profile;
pub mod protocol;
pub mod scanner;

pub use device::IrDeviceHandle;
pub use driver::IrDriver;
pub use emitter::{IrEmitter, LircEmitter, MockIrEmitter, NetworkBridgeEmitter};
pub use error::IrError;
pub use profile::{all_default_profiles, default_samsung_profile, IrProfile};
pub use protocol::{IrProtocol, IrSignal};
#[cfg(feature = "cec")]
pub use scanner::CecFeedback;
pub use scanner::{IrScanFeedback, MockFeedback, TcpProbeFeedback, UniversalScanner};
