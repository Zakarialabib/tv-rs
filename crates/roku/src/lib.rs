//! `tv-rs-roku` — Roku OS External Control Protocol (ECP) client.
//!
//! Roku devices expose the official, publicly documented, **unauthenticated**
//! ECP REST API on TCP port [`ROKU_ECP_PORT`] (8060). Requests originating on
//! the local subnet need no credentials. This crate wraps the Phase-1 subset:
//!
//! * `GET /query/device-info` → [`RokuDeviceInfo`]
//! * `GET /query/apps` → [`RokuApp`]
//! * `POST /keypress/{Key}` → remote-key presses via [`RokuKey`]
//! * `POST /launch/{appId}` → application launch
//!
//! All network-free logic (XML parsing, path and key formatting) is exposed as
//! pure functions so it can be tested without a device.
//!
//! # Example
//!
//! ```no_run
//! use tv_rs_roku::{RokuClient, RokuError, RokuKey};
//!
//! # async fn run() -> Result<(), RokuError> {
//! let client = RokuClient::new("192.168.1.50");
//! let info = client.query_device_info().await?;
//! println!("model: {:?}", info.model_name);
//!
//! let id = client.query_app_id("Netflix").await?;
//! client.launch_app(&id).await?;
//! client.send_keypress(RokuKey::Home).await?;
//! # Ok(())
//! # }
//! ```

pub mod client;
pub mod error;
pub mod payload;
pub mod protocol;
pub mod types;

pub use client::{keypress_path, launch_path, parse_apps, parse_device_info, RokuClient};
pub use error::{Result, RokuError};
pub use payload::{RokuApp, RokuDeviceInfo};
pub use protocol::EcpRestClient;
pub use types::{
    RokuKey, RokuPowerMode, ECP_APPS_PATH, ECP_DEVICE_INFO_PATH, ECP_KEYPRESS_PATH_PREFIX,
    ECP_LAUNCH_PATH_PREFIX, ROKU_ECP_PORT,
};
