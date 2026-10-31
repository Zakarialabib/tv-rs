//! Vendor classification and the discovery result type.
//!
//! This is the layer that turns raw SSDP responses into something a caller can
//! act on: *which vendor is this, and how do I talk to it?*

use crate::client::TvClient;
use crate::error::Result;
use serde::{Deserialize, Serialize};
use tv_rs_core::DeviceId;
use tv_rs_discovery::{SsdpDeviceDescriptor, SsdpMsearchResponse};
use tv_rs_roku::ROKU_ECP_PORT;
use tv_rs_sicp::SICP_TCP_PORT;
use tv_rs_tizen::TIZEN_WS_PORT;
use tv_rs_webos::WEBOS_WS_PORT;

/// The television platform a discovered device runs.
///
/// Identification is derived from the UPnP device descriptor and the SSDP
/// response headers — see [`identify_vendor`](crate::identify_vendor).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TvVendor {
    /// Samsung Tizen: WebSocket remote channel (8001/8002) plus DIAL (8080).
    Samsung,
    /// LG webOS: SSAP JSON-RPC over WebSocket (3000/3001).
    Lg,
    /// Roku OS: unauthenticated ECP REST (8060).
    Roku,
    /// Philips SICP: commercial display protocol over TCP (5000).
    Philips,
    /// Google Cast or Chromecast.
    GoogleCast,
    /// The device answered SSDP but matched no known vendor signature.
    Unknown,
}

impl TvVendor {
    /// A short, stable, human-readable name.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Samsung => "samsung",
            Self::Lg => "lg",
            Self::Roku => "roku",
            Self::Philips => "philips",
            Self::GoogleCast => "googlecast",
            Self::Unknown => "unknown",
        }
    }

    /// The TCP port the vendor's local control protocol listens on.
    ///
    /// This is the *primary* control port, used for liveness probing. Samsung
    /// and LG also expose TLS variants (`8002` / `3001`) which the protocol
    /// crates select via their own builders. `Unknown` has no control port.
    ///
    /// `TIZEN_WS_PORT`, `WEBOS_WS_PORT`, `ROKU_ECP_PORT` and `SICP_TCP_PORT` are
    /// re-exported from the matching protocol crate rather than duplicated as literals.
    pub fn control_port(&self) -> Option<u16> {
        match self {
            Self::Samsung => Some(TIZEN_WS_PORT),
            Self::Lg => Some(WEBOS_WS_PORT),
            Self::Roku => Some(ROKU_ECP_PORT),
            Self::Philips => Some(SICP_TCP_PORT),
            Self::GoogleCast => Some(8009),
            Self::Unknown => None,
        }
    }
}

/// A television located on the local network.
///
/// Produced by [`discover`](crate::discover) and friends. Call
/// [`DiscoveredTv::to_client`] to get a control handle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveredTv {
    /// The classified vendor, or [`TvVendor::Unknown`].
    pub vendor: TvVendor,
    /// IP address the SSDP reply came from.
    pub ip: String,
    /// The `LOCATION` URL of the UPnP device descriptor, when the reply had one.
    pub location: Option<String>,
    /// `friendlyName` from the device descriptor.
    pub friendly_name: Option<String>,
    /// `manufacturer` from the device descriptor.
    pub manufacturer: Option<String>,
    /// `modelName` from the device descriptor.
    pub model_name: Option<String>,
    /// `serialNumber` from the device descriptor.
    pub serial_number: Option<String>,
    /// The `USN` header of the SSDP reply.
    pub usn: Option<String>,
    /// The hardware address, when something actually reported one.
    ///
    /// An SSDP reply never carries it, so this is `None` after a scan. A direct
    /// probe can fill it (Roku's `device-info` does), which is what lets a caller
    /// pre-fill wake without asking the user to type a MAC.
    pub mac: Option<String>,
}

impl DiscoveredTv {
    /// Assemble a discovery result from an SSDP reply and its optional
    /// descriptor. Missing descriptor fields are simply left `None`.
    pub fn new(
        vendor: TvVendor,
        response: &SsdpMsearchResponse,
        descriptor: Option<&SsdpDeviceDescriptor>,
    ) -> Self {
        Self {
            vendor,
            ip: response.source.clone(),
            location: response.location.clone(),
            friendly_name: descriptor.and_then(|d| d.friendly_name.clone()),
            manufacturer: descriptor.and_then(|d| d.manufacturer.clone()),
            model_name: descriptor.and_then(|d| d.model_name.clone()),
            serial_number: descriptor.and_then(|d| d.serial_number.clone()),
            usn: response.usn.clone(),
            // An SSDP reply carries no hardware address; only a probe can supply one.
            mac: None,
        }
    }

    /// A result for an address that answered a **direct probe** rather than SSDP.
    ///
    /// There is no `M-SEARCH` reply and no descriptor, so only the vendor and the
    /// address are known. Every descriptor field stays `None` until something
    /// fetches it — which is why a probe is the cheap question, not a substitute
    /// for discovery.
    #[must_use]
    pub fn probed(vendor: TvVendor, ip: impl Into<String>) -> Self {
        Self {
            vendor,
            ip: ip.into(),
            location: None,
            friendly_name: None,
            manufacturer: None,
            model_name: None,
            serial_number: None,
            usn: None,
            mac: None,
        }
    }

    /// The best available human-readable label: friendly name, else model, else
    /// the IP address. Never empty.
    pub fn label(&self) -> &str {
        self.friendly_name
            .as_deref()
            .or(self.model_name.as_deref())
            .unwrap_or(self.ip.as_str())
    }

    /// The strongest stable identity available for this device.
    ///
    /// Identity is what a fleet dashboard keys on, so the precedence favours
    /// *uniqueness* over meaning:
    ///
    /// 1. **`usn`** — the SSDP USN embeds the device's UDN (a per-device UUID):
    ///    globally unique, survives reboots and DHCP changes, and present after
    ///    any scan that reached the descriptor.
    /// 2. **`serial_number`** — unique when the vendor is honest; some ship a
    ///    model-wide default, which is why it does not outrank the UDN.
    /// 3. **`mac`** — hardware truth, but only a direct probe ever supplies one
    ///    (an SSDP reply carries no hardware address).
    /// 4. **`ip`** — last resort: stable only for a static-IP install, and the
    ///    only answer a [`DiscoveredTv::probed`] result can give.
    ///
    /// The key is therefore *path-dependent by design*: the same television
    /// answers with its USN after a scan and with its address after a bare
    /// probe, because the probe never asked for a descriptor. Callers that need
    /// one identity across both paths should scan, not probe.
    ///
    /// This is deliberately **not** [`DeviceHandle::id`](tv_rs_core::DeviceHandle::id):
    /// that trait method lives on a connected client and keys by host, while
    /// this method exists for inventory — before any connection is made.
    #[must_use]
    pub fn stable_key(&self) -> String {
        self.usn
            .as_deref()
            .or(self.serial_number.as_deref())
            .or(self.mac.as_deref())
            .unwrap_or(self.ip.as_str())
            .to_string()
    }

    /// This device as a [`DeviceId`], for callers that want the core type.
    ///
    /// Falls back to [`DeviceId::anonymous`] only if the derived key is empty
    /// after trimming — which [`Self::stable_key`] cannot produce (every
    /// fallback arm ends in `ip`, and discovery never returns an empty source).
    #[must_use]
    pub fn device_id(&self) -> DeviceId {
        DeviceId::new(self.stable_key()).unwrap_or_else(|_| DeviceId::anonymous())
    }

    /// Build the default control client for this device.
    ///
    /// Returns [`TvError::UnsupportedDevice`](crate::TvError::UnsupportedDevice)
    /// when the vendor is [`TvVendor::Unknown`].
    pub fn to_client(&self) -> Result<TvClient> {
        TvClient::from_discovered(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(vendor: TvVendor) -> DiscoveredTv {
        DiscoveredTv {
            vendor,
            ip: "192.168.1.10".into(),
            mac: None,
            location: None,
            friendly_name: None,
            manufacturer: None,
            model_name: None,
            serial_number: None,
            usn: None,
        }
    }

    #[test]
    fn control_ports_come_from_the_protocol_crates() {
        assert_eq!(TvVendor::Samsung.control_port(), Some(8001));
        assert_eq!(TvVendor::Lg.control_port(), Some(3000));
        assert_eq!(TvVendor::Roku.control_port(), Some(8060));
        assert_eq!(TvVendor::Philips.control_port(), Some(5000));
        assert_eq!(TvVendor::Unknown.control_port(), None);
    }

    #[test]
    fn label_falls_back_from_friendly_name_to_model_to_ip() {
        let mut tv = device(TvVendor::Roku);
        assert_eq!(tv.label(), "192.168.1.10");

        tv.model_name = Some("Roku Ultra".into());
        assert_eq!(tv.label(), "Roku Ultra");

        tv.friendly_name = Some("Living Room".into());
        assert_eq!(tv.label(), "Living Room");
    }

    #[test]
    fn vendor_names_are_stable() {
        assert_eq!(TvVendor::Samsung.as_str(), "samsung");
        assert_eq!(TvVendor::Lg.as_str(), "lg");
        assert_eq!(TvVendor::Roku.as_str(), "roku");
        assert_eq!(TvVendor::Philips.as_str(), "philips");
        assert_eq!(TvVendor::Unknown.as_str(), "unknown");
    }

    /// The precedence the fleet dashboard keys on: uniqueness beats meaning,
    /// and a bare probe degrades to its address rather than to nothing.
    #[test]
    fn stable_key_prefers_usn_then_serial_then_mac_then_ip() {
        let mut tv = device(TvVendor::Samsung);
        assert_eq!(
            tv.stable_key(),
            "192.168.1.10",
            "a probed result has only its address"
        );

        tv.mac = Some("AA:BB:CC:DD:EE:FF".into());
        assert_eq!(tv.stable_key(), "AA:BB:CC:DD:EE:FF");

        tv.serial_number = Some("SN-1234".into());
        assert_eq!(tv.stable_key(), "SN-1234");

        tv.usn = Some("uuid:7de5f0a1-0000-4000-8000-000000000001::urn:upnp:rootdevice".into());
        assert_eq!(
            tv.stable_key(),
            "uuid:7de5f0a1-0000-4000-8000-000000000001::urn:upnp:rootdevice",
            "the USN carries the UDN, which outranks a vendor serial"
        );
        assert_eq!(
            tv.device_id().as_str(),
            tv.stable_key(),
            "device_id is stable_key in the core type"
        );
    }

    /// Two same-model panels with a vendor-default serial must not collide:
    /// the USN wins precisely because serials can be model-wide.
    #[test]
    fn same_model_panels_with_default_serials_do_not_collide() {
        let mut a = device(TvVendor::Samsung);
        let mut b = device(TvVendor::Samsung);
        a.serial_number = Some("unknown".into());
        b.serial_number = Some("unknown".into());
        a.usn = Some("uuid:aaaa::urn:upnp:rootdevice".into());
        b.usn = Some("uuid:bbbb::urn:upnp:rootdevice".into());
        assert_ne!(a.stable_key(), b.stable_key());
    }
}
