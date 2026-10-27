//! `tv-rs-discovery` — Universal display discovery, device fingerprinting, and device registry.
//!
//! This crate provides universal discovery of televisions, commercial panels, streaming boxes,
//! and digital signage displays across multiple protocols:
//! - **SSDP / UPnP** (`SsdpScanner`)
//! - **mDNS / Bonjour** (`MdnsScanner`, `MdnsAdvertiser`)
//! - **ADB Network Devices** (`AdbScanner`)
//! - **Bluetooth LE Advertising** (`BluetoothScanner`)
//! - **Google Cast Devices** (`CastScanner`)
//! - **Local HTTP Display APIs** (`LocalHttpDisplayScanner`)
//! - **HDMI-CEC Topology** (`CecTopologyScanner`)
//! - **Saved Device Registry** (`DeviceRegistry`)
//!
//! # Device Fingerprinting
//! The [`DeviceFingerprint`] combines signatures (MAC pattern, mDNS hostname, SSDP server string,
//! open ports, HDMI-CEC vendor ID, Bluetooth advertising name, ADB model string) to identify:
//! - Smart TV brands (Samsung Tizen, LG webOS, Sony Bravia, Roku TV)
//! - Android TV boxes
//! - Amlogic platform devices
//! - Amazon Fire TV devices
//! - Google Chromecast-like receivers
//! - Commercial digital signage panels (Samsung MDC, SICP, LG Commercial)
//!
//! **Discovery needs no authentication.** SSDP, mDNS, BLE advertising, and network probes are
//! unauthenticated LAN/multicast protocols used to inventory devices prior to control or pairing.
//!
//! # Example
//!
//! ```no_run
//! use tv_rs_discovery::SsdpScanner;
//!
//! # async fn example() -> Result<(), tv_rs_discovery::DiscoveryError> {
//! let scanner = SsdpScanner::new();
//!
//! for response in scanner.discover().await? {
//!     if let Some(location) = response.location {
//!         println!("{} -> {}", response.source, location);
//!     }
//! }
//! # Ok(())
//! # }
//! ```

#![forbid(unsafe_code)]
pub mod adb;
pub mod bluetooth;
pub mod cast;
pub mod cec_topology;
pub mod client;
pub mod error;
pub mod fingerprint;
pub mod http_display;
pub mod mdns;
pub mod pairing;
pub mod payload;
pub mod protocol;
pub mod registry;
pub mod types;
pub mod universal;

pub use adb::{AdbDevice, AdbScanner};
pub use bluetooth::{BluetoothAdvertisement, BluetoothScanner};
pub use cast::{CastDevice, CastScanner, CAST_MDNS_SERVICE_TYPE};
pub use cec_topology::{vendor_id_to_name, CecDeviceNode, CecTopologyMap, CecTopologyScanner};
pub use fingerprint::DeviceFingerprint;
pub use http_display::{HttpDisplay, LocalHttpDisplayScanner, HTTP_DISPLAY_PORTS};
pub use registry::{DeviceRegistry, SavedDevice};
pub use universal::{DiscoveredDevice, UniversalScanner};

pub use client::{build_msearch, parse_device_descriptor, parse_msearch_response, SsdpScanner};
pub use error::{DiscoveryError, Result};
pub use mdns::{
    MdnsAdvertiser, MdnsScanner, MdnsServiceRecord, MDNS_SERVICE_TYPE_SIGNAGE, MDNS_SERVICE_TYPE_TV,
};
pub use pairing::{
    validate_handshake, SignageHandshakeRequest, SignageHandshakeResponse, SignagePairingPayload,
    SignagePairingSession, DEFAULT_SESSION_TTL_SECS, SIGNAGE_PAIRING_URI_SCHEME,
};
pub use payload::{SsdpDeviceDescriptor, SsdpMsearchResponse};
pub use protocol::SsdpDiscovery;
pub use types::{
    DiscoveryMethod, MDNS_MULTICAST_IP, MDNS_UDP_PORT, SSDP_ALL_DEVICES, SSDP_MULTICAST_IP,
    SSDP_ROOT_DEVICE, SSDP_UDP_PORT,
};
