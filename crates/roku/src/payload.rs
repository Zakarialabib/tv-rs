//! Protocol payload structs (serde).
//!
//! These model the XML documents returned by the Roku ECP `/query` endpoints.
//! XML is parsed with `roxmltree` (see `client::parse_device_info` and
//! `client::parse_apps`); the serde derives mirror the same shapes so callers
//! can round-trip them through JSON if they wish.

use serde::{Deserialize, Serialize};

/// A parsed `GET /query/device-info` response body.
///
/// Every field is optional because different Roku models and firmware versions
/// expose different subsets of `<device-info>` elements. Hyphenated XML element
/// names (`<vendor-name>`, `<supports-suspend>`, …) map to the underscore field
/// names below.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct RokuDeviceInfo {
    /// Manufacturer, typically `"Roku"`. From `<vendor-name>`.
    pub vendor_name: Option<String>,
    /// Marketing model name, e.g. `"Roku Ultra"`. From `<model-name>`.
    pub model_name: Option<String>,
    /// Model number, e.g. `"4800X"`. From `<model-number>`.
    pub model_number: Option<String>,
    /// Device serial number. From `<serial-number>`.
    pub serial_number: Option<String>,
    /// Unique device id used by Roku cloud services. From `<device-id>`.
    pub device_id: Option<String>,
    /// Wi-Fi MAC address. From `<wifi-mac>`.
    pub wifi_mac: Option<String>,
    /// Ethernet MAC address. From `<ethernet-mac>`.
    pub ethernet_mac: Option<String>,
    /// Current power state, e.g. `"PowerOn"`. From `<power-mode>`.
    pub power_mode: Option<String>,
    /// Current output resolution, e.g. `"1080p"`. From `<screen-resolution>`.
    pub screen_resolution: Option<String>,
    /// Installed OS version. From `<software-version>`.
    pub software_version: Option<String>,
    /// User-assigned device name. From `<user-device-name>`.
    pub user_device_name: Option<String>,
    /// Whether the device reports support for suspend. From `<supports-suspend>`.
    pub supports_suspend: Option<bool>,
    /// Whether the device reports support for find-remote. From `<supports-find-remote>`.
    pub supports_find_remote: Option<bool>,
}

/// A single entry from a `GET /query/apps` response.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RokuApp {
    /// The application id taken from the `id` attribute.
    pub id: String,
    /// The human-readable application name taken from the element text.
    pub name: String,
}
