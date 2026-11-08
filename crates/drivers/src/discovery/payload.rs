//! Serde payload structs for SSDP responses and UPnP device descriptors.

use serde::{Deserialize, Serialize};

/// A parsed SSDP `M-SEARCH` response datagram.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SsdpMsearchResponse {
    /// `LOCATION` header: URL of the UPnP device descriptor document.
    pub location: Option<String>,
    /// `SERVER` header: identifies the OS, UPnP version and product.
    pub server: Option<String>,
    /// `USN` header: unique service name identifying the device or service.
    pub usn: Option<String>,
    /// `ST` header: search target the response is answering.
    pub st: Option<String>,
    /// `CACHE-CONTROL` header: freshness lifetime of the advertisement.
    pub cache_control: Option<String>,
    /// IP address of the peer the datagram came from.
    pub source: String,
}

/// The subset of a UPnP device descriptor this crate cares about.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SsdpDeviceDescriptor {
    /// `<friendlyName>`: human-readable device name.
    pub friendly_name: Option<String>,
    /// `<manufacturer>`: device vendor.
    pub manufacturer: Option<String>,
    /// `<modelName>`: marketing model name.
    pub model_name: Option<String>,
    /// `<modelNumber>`: model identifier.
    pub model_number: Option<String>,
    /// `<UDN>`: unique device name (usually a `uuid:` value).
    pub udn: Option<String>,
    /// `<deviceType>`: UPnP device type URN.
    pub device_type: Option<String>,
    /// `<serialNumber>`: device serial number.
    pub serial_number: Option<String>,
}
