//! Vendor identification, discovery orchestration and the aggregate [`TvClient`].
//!
//! The facade owns no wire protocol of its own: it classifies a discovered
//! device, hands it to the matching protocol crate, and forwards calls. Anything
//! vendor-specific stays in `tv-rs-tizen` / `tv-rs-webos` / `tv-rs-roku` and is
//! reachable through the typed accessors on [`TvClient`].

use crate::error::{Result, TvError};
use crate::types::{DiscoveredTv, TvVendor};
use tv_rs_discovery::{SsdpDeviceDescriptor, SsdpMsearchResponse, SsdpScanner};
use tv_rs_roku::RokuClient;
use tv_rs_tizen::{TizenClient, TizenRemoteKey, TizenSession};
use tv_rs_webos::WebosClient;

/// Classify a discovered device by its SSDP reply and UPnP descriptor.
///
/// The match is a documented **heuristic** over the free-text fields a device
/// advertises (`SERVER`, `ST`, `USN`, `manufacturer`, `modelName`, ...) — SSDP
/// carries no authoritative vendor identifier. Recognition is therefore
/// deliberately narrow: a device that advertises none of the signatures below
/// is reported as [`TvVendor::Unknown`] rather than guessed at.
///
/// Signatures, checked in order:
///
/// * **Roku** — `roku` (its `SERVER` header is `Roku/...` and its search target
///   is `roku:ecp`).
/// * **Samsung** — `samsung` or `tizen`.
/// * **LG** — `webos` or `lg electronics`.
pub fn identify_vendor(
    response: &SsdpMsearchResponse,
    descriptor: Option<&SsdpDeviceDescriptor>,
) -> TvVendor {
    let mut haystack = String::new();

    for header in [&response.server, &response.st, &response.usn] {
        if let Some(value) = header {
            haystack.push_str(value);
            haystack.push(' ');
        }
    }
    if let Some(descriptor) = descriptor {
        for value in [
            &descriptor.manufacturer,
            &descriptor.friendly_name,
            &descriptor.model_name,
            &descriptor.model_number,
            &descriptor.device_type,
            &descriptor.udn,
        ] {
            if let Some(value) = value {
                haystack.push_str(value);
                haystack.push(' ');
            }
        }
    }

    let haystack = haystack.to_ascii_lowercase();

    if haystack.contains("roku") {
        TvVendor::Roku
    } else if haystack.contains("samsung") || haystack.contains("tizen") {
        TvVendor::Samsung
    } else if haystack.contains("webos") || haystack.contains("lg electronics") {
        TvVendor::Lg
    } else {
        TvVendor::Unknown
    }
}

/// Discover televisions on the local network and classify each one.
///
/// Shorthand for [`discover_with`] using [`SsdpScanner::new`]. Devices whose
/// `LOCATION` is missing or whose descriptor cannot be fetched are skipped by
/// the underlying scanner.
///
/// # Errors
///
/// Returns [`TvError::Discovery`] if the SSDP socket cannot be opened or a
/// reply cannot be parsed.
pub async fn discover() -> Result<Vec<DiscoveredTv>> {
    discover_with(&SsdpScanner::new()).await
}

/// Like [`discover`], but with a caller-configured [`SsdpScanner`].
///
/// Use this to shorten the reply window (`with_timeout`) or to pin the network
/// interface on a multi-homed host.
///
/// # Errors
///
/// Returns [`TvError::Discovery`] if the SSDP socket cannot be opened or a
/// reply cannot be parsed.
pub async fn discover_with(scanner: &SsdpScanner) -> Result<Vec<DiscoveredTv>> {
    let found = scanner.discover_tvs().await?;

    Ok(found
        .into_iter()
        .map(|(response, descriptor)| {
            let vendor = identify_vendor(&response, Some(&descriptor));
            DiscoveredTv::new(vendor, &response, Some(&descriptor))
        })
        .collect())
}

/// A control handle for a discovered television.
///
/// The variant is chosen from [`DiscoveredTv::vendor`]. The convenience methods
/// ([`power_off`](TvClient::power_off), [`launch_app`](TvClient::launch_app))
/// cover only the operations that have an equivalent on all three vendors;
/// everything else goes through the typed accessors.
#[derive(Debug, Clone)]
pub enum TvClient {
    /// Samsung Tizen: remote channel plus DIAL.
    Tizen(TizenClient),
    /// LG webOS: SSAP over WebSocket.
    Webos(WebosClient),
    /// Roku OS: ECP REST.
    Roku(RokuClient),
}

impl TvClient {
    /// Build the default control client for a discovered device.
    ///
    /// # Errors
    ///
    /// Returns [`TvError::UnsupportedVendor`] when the device is
    /// [`TvVendor::Unknown`], since no protocol crate can drive it.
    pub fn from_discovered(tv: &DiscoveredTv) -> Result<Self> {
        match tv.vendor {
            TvVendor::Samsung => Ok(Self::Tizen(TizenClient::new(tv.ip.clone()))),
            TvVendor::Lg => Ok(Self::Webos(WebosClient::new(tv.ip.clone()))),
            TvVendor::Roku => Ok(Self::Roku(RokuClient::new(tv.ip.clone()))),
            TvVendor::Unknown => Err(TvError::UnsupportedVendor {
                ip: tv.ip.clone(),
                reason: "device did not advertise a known vendor signature".to_string(),
            }),
        }
    }

    /// The vendor this handle targets.
    pub fn vendor(&self) -> TvVendor {
        match self {
            Self::Tizen(_) => TvVendor::Samsung,
            Self::Webos(_) => TvVendor::Lg,
            Self::Roku(_) => TvVendor::Roku,
        }
    }

    /// The address the underlying client targets, for logging and diagnostics.
    ///
    /// Tizen and webOS report the bare host; Roku's ECP client reports its full
    /// base URL (`http://host:8060`). Use [`DiscoveredTv::ip`] when you need the
    /// address in a uniform form.
    pub fn address(&self) -> &str {
        match self {
            Self::Tizen(client) => client.host(),
            Self::Webos(client) => client.host(),
            Self::Roku(client) => client.base_url(),
        }
    }

    /// The Tizen client, when this handle targets a Samsung TV.
    pub fn as_tizen(&self) -> Option<&TizenClient> {
        match self {
            Self::Tizen(client) => Some(client),
            _ => None,
        }
    }

    /// The webOS client, when this handle targets an LG TV.
    pub fn as_webos(&self) -> Option<&WebosClient> {
        match self {
            Self::Webos(client) => Some(client),
            _ => None,
        }
    }

    /// The Roku client, when this handle targets a Roku device.
    pub fn as_roku(&self) -> Option<&RokuClient> {
        match self {
            Self::Roku(client) => Some(client),
            _ => None,
        }
    }

    /// Turn the television off.
    ///
    /// Tizen and webOS require an open session, so this opens (and drops) one
    /// per call; Roku's ECP endpoint is stateless.
    ///
    /// # Errors
    ///
    /// Returns the originating protocol crate's error, wrapped in [`TvError`].
    pub async fn power_off(&self) -> Result<()> {
        match self {
            Self::Tizen(client) => {
                let mut session = TizenSession::connect(client).await?;
                session.send_key(TizenRemoteKey::PowerOff).await?;
                Ok(())
            }
            Self::Webos(client) => {
                let mut session = client.connect().await?;
                session.power_off().await?;
                Ok(())
            }
            Self::Roku(client) => {
                client.power_off().await?;
                Ok(())
            }
        }
    }

    /// Launch an installed application by id.
    ///
    /// The id is vendor-specific: a DIAL app id on Tizen, an SSAP app id on
    /// webOS, and an ECP app id on Roku. Use the typed accessors when you need
    /// to resolve a human-readable name first (see
    /// [`RokuClient::query_app_id`]).
    ///
    /// # Errors
    ///
    /// Returns the originating protocol crate's error, wrapped in [`TvError`].
    pub async fn launch_app(&self, app_id: &str) -> Result<()> {
        match self {
            Self::Tizen(client) => {
                client.launch_app_via_dial(app_id, None).await?;
                Ok(())
            }
            Self::Webos(client) => {
                let mut session = client.connect().await?;
                session.launch_app(app_id).await?;
                Ok(())
            }
            Self::Roku(client) => {
                client.launch_app(app_id).await?;
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(server: &str, st: &str) -> SsdpMsearchResponse {
        SsdpMsearchResponse {
            server: Some(server.to_string()),
            st: Some(st.to_string()),
            source: "192.168.1.10".to_string(),
            ..SsdpMsearchResponse::default()
        }
    }

    fn descriptor(manufacturer: &str, model: &str) -> SsdpDeviceDescriptor {
        SsdpDeviceDescriptor {
            manufacturer: Some(manufacturer.to_string()),
            model_name: Some(model.to_string()),
            ..SsdpDeviceDescriptor::default()
        }
    }

    #[test]
    fn identifies_each_vendor_from_its_ssdp_signature() {
        assert_eq!(
            identify_vendor(
                &response("Roku/9.4 UPnP/1.0 Roku/9.4", "roku:ecp"),
                Some(&descriptor("Roku", "Roku Ultra"))
            ),
            TvVendor::Roku
        );
        assert_eq!(
            identify_vendor(
                &response("Linux/2.6 UPnP/1.0 Samsung UPnP SDK/1.0", "upnp:rootdevice"),
                Some(&descriptor("Samsung Electronics", "UE55TU8000"))
            ),
            TvVendor::Samsung
        );
        assert_eq!(
            identify_vendor(
                &response("WebOS/1.0 UPnP/1.0 LG Electronics", "upnp:rootdevice"),
                None
            ),
            TvVendor::Lg
        );
    }

    #[test]
    fn unmatched_devices_are_reported_as_unknown_not_guessed() {
        assert_eq!(
            identify_vendor(&response("SomeVendor/1.0", "upnp:rootdevice"), None),
            TvVendor::Unknown
        );
    }

    #[test]
    fn unknown_vendors_cannot_produce_a_client() {
        let tv = DiscoveredTv {
            vendor: TvVendor::Unknown,
            ip: "192.168.1.10".to_string(),
            location: None,
            friendly_name: None,
            manufacturer: None,
            model_name: None,
            serial_number: None,
            usn: None,
        };

        let error = TvClient::from_discovered(&tv).unwrap_err();
        assert!(matches!(error, TvError::UnsupportedVendor { .. }));
    }

    #[test]
    fn clients_target_the_discovered_ip() {
        for (vendor, expected) in [
            (TvVendor::Samsung, "192.168.1.10"),
            (TvVendor::Lg, "192.168.1.10"),
            (TvVendor::Roku, "http://192.168.1.10:8060"),
        ] {
            let tv = DiscoveredTv {
                vendor,
                ip: "192.168.1.10".to_string(),
                location: None,
                friendly_name: None,
                manufacturer: None,
                model_name: None,
                serial_number: None,
                usn: None,
            };

            let client = tv.to_client().unwrap();
            assert_eq!(client.vendor(), vendor);
            assert_eq!(client.address(), expected);
        }
    }
}
