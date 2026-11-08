//! Roku ECP protocol driver and client types.

pub mod client;
pub mod driver;
pub mod error;
pub mod payload;
pub mod protocol;
pub mod types;

pub use client::{
    keypress_path, launch_path, parse_active_app, parse_apps, parse_device_info, RokuClient,
};
pub use driver::{map_core_key_to_roku, RokuDeviceHandle, RokuDriver};
pub use error::{Result, RokuError};
pub use payload::{RokuActiveApp, RokuApp, RokuDeviceInfo};
pub use protocol::EcpRestClient;
pub use types::{
    RokuKey, RokuPowerMode, ECP_ACTIVE_APP_PATH, ECP_APPS_PATH, ECP_DEVICE_INFO_PATH,
    ECP_KEYPRESS_PATH_PREFIX, ECP_LAUNCH_PATH_PREFIX, ROKU_ECP_PORT,
};
