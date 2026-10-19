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
}
