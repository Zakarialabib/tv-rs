//! Vendor classification and the discovery result type.
//!
//! This is the layer that turns raw SSDP responses into something a caller can
//! act on: *which vendor is this, and how do I talk to it?*

use crate::client::TvClient;
use crate::error::Result;
use serde::{Deserialize, Serialize};
use tv_rs_discovery::{SsdpDeviceDescriptor, SsdpMsearchResponse};
use tv_rs_roku::ROKU_ECP_PORT;
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
            Self::Unknown => "unknown",
        }
    }

    /// The TCP port the vendor's local control protocol listens on.
    ///
    /// This is the *primary* control port, used for liveness probing. Samsung
    /// and LG also expose TLS variants (`8002` / `3001`) which the protocol
    /// crates select via their own builders. `Unknown` has no control port.
    ///
    /// `TIZEN_WS_PORT`, `WEBOS_WS_PORT` and `ROKU_ECP_PORT` are re-exported
    /// from the matching protocol crate rather than duplicated as literals.
    pub fn control_port(&self) -> Option<u16> {
        match self {
            Self::Samsung => Some(TIZEN_WS_PORT),
            Self::Lg => Some(WEBOS_WS_PORT),
            Self::Roku => Some(ROKU_ECP_PORT),
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
        assert_eq!(TvVendor::Unknown.as_str(), "unknown");
    }
}
