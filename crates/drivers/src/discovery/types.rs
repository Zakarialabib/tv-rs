//! Constants and enums shared across the discovery crate.

use serde::{Deserialize, Serialize};

/// IPv4 multicast address used by SSDP.
pub const SSDP_MULTICAST_IP: &str = "239.255.255.250";

/// UDP port used by SSDP.
pub const SSDP_UDP_PORT: u16 = 1900;

/// IPv4 multicast address used by mDNS.
pub const MDNS_MULTICAST_IP: &str = "224.0.0.251";

/// UDP port used by mDNS.
pub const MDNS_UDP_PORT: u16 = 5353;

/// SSDP search target that matches every device.
pub const SSDP_ALL_DEVICES: &str = "ssdp:all";

/// SSDP search target that matches root devices only.
pub const SSDP_ROOT_DEVICE: &str = "upnp:rootdevice";

/// A discovery mechanism supported by this crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiscoveryMethod {
    /// SSDP over UDP multicast (UPnP, Samsung Tizen, LG webOS).
    Ssdp,
    /// mDNS over UDP multicast.
    Mdns,
    /// Bonjour, Apple's mDNS-based discovery.
    Bonjour,
    /// ARP table scanning to map an IP address to a MAC address.
    Arp,
    /// ADB over TCP network discovery (port 5555).
    Adb,
    /// Bluetooth LE advertising scan.
    Bluetooth,
    /// Google Cast / Chromecast service discovery.
    Cast,
    /// Local HTTP / REST / MDC / SICP web API display probe.
    LocalHttp,
    /// HDMI-CEC topology bus scanning.
    CecTopology,
}

/// Category or brand classification of a discovered display/device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiscoveredDeviceKind {
    /// Consumer Smart TV with brand name (e.g. Samsung Tizen, LG webOS, Sony Bravia, Roku TV).
    TvBrand(String),
    /// Android TV box or streaming device (e.g. Nvidia Shield, generic Android TV).
    AndroidBox(String),
    /// Amlogic platform device (reference board, OTT set-top-box).
    AmlogicDevice(String),
    /// Amazon Fire TV device (Fire TV stick, cube, Fire Edition TV).
    FireTv(String),
    /// Google Chromecast or Cast receiver device.
    ChromecastLike(String),
    /// Commercial or industrial digital signage display panel (Samsung MDC, LG Signage, SICP, NEC).
    CommercialPanel(String),
    /// Generic or unclassified display.
    Unknown(String),
}
