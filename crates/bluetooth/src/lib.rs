//! Bluetooth HID keyboard and BLE remote control emulation for TV devices.
//!
//! Modern streaming devices and smart TVs (such as Apple TV, Fire TV, Android TV,
//! and modern Samsung SolarCell remotes) use Bluetooth or BLE rather than IR.
//! By emulating a Bluetooth HID keyboard or gamepad, `tv-rs-bluetooth` enables
//! sending standard key navigation, media controls, and typing text directly to
//! TV targets.
//!
//! ```no_run
//! use std::sync::Arc;
//! use tv_rs_bluetooth::{BluetoothAddress, BluetoothRemote, MockBluetoothTransport};
//! use tv_rs_core::{CommandId, Key, DeviceHandle};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), tv_rs_core::TvError> {
//!     let addr = BluetoothAddress::try_from("AA:BB:CC:DD:EE:FF")?;
//!     let transport = MockBluetoothTransport::new();
//!     let remote = BluetoothRemote::new(addr, transport);
//!
//!     if let Some(key_input) = remote.as_key() {
//!         key_input.key_press(Key::Select, CommandId::new()).await?;
//!     }
//!     Ok(())
//! }
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod device;
pub mod error;
pub mod hid;
pub mod transport;

pub use device::BluetoothRemote;
pub use error::BluetoothError;
pub use hid::{map_ascii_char_to_hid, map_key_to_hid, HidKeyboardReport, HidKeycode};
pub use transport::{
    BluetoothAddress, BluetoothCtlTransport, BluetoothTransport, MockBluetoothTransport,
};
