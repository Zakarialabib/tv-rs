//! Vendor identification, discovery orchestration and the aggregate [`TvClient`].
//!
//! The facade owns no wire protocol of its own: it classifies a discovered
//! device, hands it to the matching protocol crate, and forwards calls. Anything
//! vendor-specific stays in `tv-rs-tizen` / `tv-rs-webos` / `tv-rs-roku` and is
//! reachable through the typed accessors on [`TvClient`].

use std::fmt;
use std::sync::Arc;

use crate::error::{Result, TvError};
use crate::map;
use crate::types::{DiscoveredTv, TvVendor};
use tv_rs_core::{Capability, ObservedField};
use tv_rs_discovery::{SsdpDeviceDescriptor, SsdpMsearchResponse, SsdpScanner};
use tv_rs_power::PowerState;
use tv_rs_roku::{RokuClient, RokuPowerMode};
use tv_rs_tizen::{DynConnectionPool, LiveChannelFactory, TizenClient, TizenRemoteKey};
use tv_rs_transport::TlsPolicy;
use tv_rs_webos::{DynSessionPool, LiveSessionFactory, WebosClient};

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

    for value in [&response.server, &response.st, &response.usn]
        .into_iter()
        .flatten()
    {
        haystack.push_str(value);
        haystack.push(' ');
    }
    if let Some(descriptor) = descriptor {
        for value in [
            &descriptor.manufacturer,
            &descriptor.friendly_name,
            &descriptor.model_name,
            &descriptor.model_number,
            &descriptor.device_type,
            &descriptor.udn,
        ]
        .into_iter()
        .flatten()
        {
            haystack.push_str(value);
            haystack.push(' ');
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
/// Returns [`TvError::Protocol`] if the SSDP socket cannot be opened or a
/// reply cannot be parsed (discovery errors map through [`map::discovery`]).
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
/// Returns the [`tv_rs_core::TvError`] taxonomy the other facade calls return
/// (see [`map::discovery`] for how discovery errors are classified).
pub async fn discover_with(scanner: &SsdpScanner) -> Result<Vec<DiscoveredTv>> {
    let found = scanner
        .discover_tvs()
        .await
        .map_err(|error| map::discovery(&error))?;

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
#[derive(Clone)]
pub enum TvClient {
    /// Samsung Tizen: remote channel plus DIAL.
    ///
    /// The pool is not optional: Samsung drops idle remote channels, so a
    /// facade that dialled per press would pay a handshake (and, with no stored
    /// token, an on-screen PIN prompt) on every key.
    Tizen {
        /// The protocol client: address, TLS policy and pairing token.
        client: TizenClient,
        /// One remote channel per TV, reopened once when a send fails.
        ///
        /// Shared behind an [`Arc`], so cloning a handle clones the *connection*,
        /// not the right to open another one.
        pool: Arc<DynConnectionPool>,
        /// The panel's hardware address, once a caller has supplied it.
        ///
        /// Samsung drops its network stack when it sleeps, so a magic packet is
        /// the only wake path — and a packet is addressed to a MAC. Neither the
        /// SSDP reply nor the pairing token carries one, so it arrives through
        /// [`TvClient::with_mac`].
        mac: Option<String>,
    },
    /// LG webOS: SSAP over WebSocket.
    ///
    /// The pool is not optional here either: connecting without a stored
    /// `client-key` raises the on-screen "Allow?" prompt, so one session per TV
    /// is what keeps a volume change from interrupting a show.
    Webos {
        /// The protocol client: address, TLS policy and client key.
        client: WebosClient,
        /// One registered SSAP session per TV, reopened once when a request fails.
        ///
        /// Shared behind an [`Arc`], so cloning a handle clones the *session*.
        pool: Arc<DynSessionPool>,
        /// The panel's hardware address, once a caller has supplied it; the Tizen
        /// variant explains why waking needs one.
        mac: Option<String>,
    },
    /// Roku OS: ECP REST.
    ///
    /// Stateless — ECP is plain HTTP with no session to keep, so there is no pool.
    Roku {
        /// The ECP client: address and base URL.
        client: RokuClient,
        /// The address `/query/device-info` reports, once it has been read.
        ///
        /// Roku wakes over ECP and needs no packet, so this is inventory rather
        /// than a lever — it is what a fleet dashboard shows as the box's identity.
        mac: Option<String>,
    },
}

/// The pools hold live sockets and have no useful `Debug` form, so the handle
/// reports which vendor it is and what it is aimed at, and stops there.
impl fmt::Debug for TvClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tizen { client, .. } => f
                .debug_struct("Tizen")
                .field("client", client)
                .finish_non_exhaustive(),
            Self::Webos { client, .. } => f
                .debug_struct("Webos")
                .field("client", client)
                .finish_non_exhaustive(),
            Self::Roku { client, .. } => f.debug_tuple("Roku").field(client).finish(),
        }
    }
}

/// The port the quirks table says this panel actually listens on, if any.
///
/// Returns `None` when the crate was built without the `quirks` feature, when no
/// row matches, or when the matching row is not about ports — in every one of those
/// cases the client's own ladder is the right answer.
#[cfg(feature = "quirks")]
fn quirk_port(vendor: &str, model: Option<&str>) -> Option<u16> {
    use tv_rs_core::quirks::{QuirkAction, Quirks};
    let table = Quirks::bundled().ok()?;
    match table.action_for(vendor, model, None)? {
        QuirkAction::StartAtPort(port) => Some(port),
        _ => None,
    }
}

#[cfg(not(feature = "quirks"))]
fn quirk_port(_vendor: &str, _model: Option<&str>) -> Option<u16> {
    None
}

impl TvClient {
    /// Build the default control client for a discovered device.
    ///
    /// # Errors
    ///
    /// Returns [`TvError::UnsupportedDevice`] when the device is
    /// [`TvVendor::Unknown`], since no protocol crate can drive it.
    pub fn from_discovered(tv: &DiscoveredTv) -> Result<Self> {
        match tv.vendor {
            TvVendor::Samsung => {
                let mut client = TizenClient::new(tv.ip.clone()).with_wss();
                if let Some(port) = quirk_port("samsung", tv.model_name.as_deref()) {
                    client = client.pinned_port(port);
                }
                Ok(Self::Tizen {
                    client,
                    pool: Arc::new(DynConnectionPool::with_factory(Box::new(
                        LiveChannelFactory,
                    ))),
                    mac: None,
                })
            }
            TvVendor::Lg => {
                let mut client = WebosClient::new(tv.ip.clone()).with_wss();
                if let Some(port) = quirk_port("lg", tv.model_name.as_deref()) {
                    client = client.pinned_port(port);
                }
                Ok(Self::Webos {
                    client,
                    pool: Arc::new(DynSessionPool::with_factory(Box::new(LiveSessionFactory))),
                    mac: None,
                })
            }
            TvVendor::Roku => Ok(Self::Roku {
                client: RokuClient::new(tv.ip.clone()),
                mac: None,
            }),
            TvVendor::Unknown => Err(TvError::UnsupportedDevice {
                address: tv.ip.clone(),
                reason: "device did not advertise a known vendor signature".to_string(),
            }),
        }
    }

    /// Builds a control client with an explicit TLS policy.
    ///
    /// This is the function that makes certificate pinning reachable from the
    /// facade: [`TvClient::from_discovered`] keeps the strict default, and a caller
    /// that has recorded a pin (after a `TlsPolicy::TrustOnFirstUse` pass) hands it
    /// in here.
    ///
    /// Roku ignores the policy — ECP is plain HTTP on the LAN.
    ///
    /// # Errors
    ///
    /// Returns [`TvError::UnsupportedDevice`] for [`TvVendor::Unknown`].
    pub fn from_discovered_with(tv: &DiscoveredTv, tls: TlsPolicy) -> Result<Self> {
        match tv.vendor {
            TvVendor::Samsung => {
                let mut client = TizenClient::new(tv.ip.clone())
                    .with_wss()
                    .with_tls_policy(tls);
                if let Some(port) = quirk_port("samsung", tv.model_name.as_deref()) {
                    client = client.pinned_port(port);
                }
                Ok(Self::Tizen {
                    client,
                    pool: Arc::new(DynConnectionPool::with_factory(Box::new(
                        LiveChannelFactory,
                    ))),
                    mac: None,
                })
            }
            TvVendor::Lg => {
                let mut client = WebosClient::new(tv.ip.clone())
                    .with_wss()
                    .with_tls_policy(tls);
                if let Some(port) = quirk_port("lg", tv.model_name.as_deref()) {
                    client = client.pinned_port(port);
                }
                Ok(Self::Webos {
                    client,
                    pool: Arc::new(DynSessionPool::with_factory(Box::new(LiveSessionFactory))),
                    mac: None,
                })
            }
            TvVendor::Roku => Ok(Self::Roku {
                client: RokuClient::new(tv.ip.clone()),
                mac: None,
            }),
            TvVendor::Unknown => Err(TvError::UnsupportedDevice {
                address: tv.ip.clone(),
                reason: "device did not advertise a known vendor signature".to_string(),
            }),
        }
    }

    /// Shows a URL on the screen.
    ///
    /// This is the signage primitive: "put this page on this panel" without
    /// installing an application.
    ///
    /// * **webOS** — `ssap://system.launcher/open`, the built-in browser.
    /// * **Tizen** — not implemented: Samsung's URL-launcher service is a B2B
    ///   feature whose API differs from the consumer remote channel, and it has not
    ///   been verified here. Returns [`TvError::UnsupportedOperation`] rather than a
    ///   silent app launch, because launching a browser with no URL is worse than
    ///   saying no.
    /// * **Roku** — no browser at all.
    ///
    /// # Errors
    ///
    /// Returns [`TvError`]; the webOS error is mapped by [`map::webos`], naming
    /// [`Capability::AppLaunch`] as the nearest capability ("put content on the
    /// panel") — the webOS path has no open_url-deferred gap to sense.
    pub async fn open_url(&self, url: &str) -> Result<()> {
        match self {
            Self::Webos { client, pool, .. } => pool
                .open_url(client, url)
                .await
                .map_err(|error| map::webos(&error, Capability::AppLaunch)),
            Self::Tizen { .. } => Err(TvError::UnsupportedOperation {
                vendor: "samsung",
                operation: "open_url",
            }),
            Self::Roku { .. } => Err(TvError::UnsupportedOperation {
                vendor: "roku",
                operation: "open_url",
            }),
        }
    }

    /// The vendor this handle targets.
    pub fn vendor(&self) -> TvVendor {
        match self {
            Self::Tizen { .. } => TvVendor::Samsung,
            Self::Webos { .. } => TvVendor::Lg,
            Self::Roku { .. } => TvVendor::Roku,
        }
    }

    /// The address the underlying client targets, for logging and diagnostics.
    ///
    /// Tizen and webOS report the bare host; Roku's ECP client reports its full
    /// base URL (`http://host:8060`). Use [`DiscoveredTv::ip`] when you need the
    /// address in a uniform form.
    pub fn address(&self) -> &str {
        match self {
            Self::Tizen { client, .. } => client.host(),
            Self::Webos { client, .. } => client.host(),
            Self::Roku { client, .. } => client.base_url(),
        }
    }

    /// The Tizen client, when this handle targets a Samsung TV.
    pub fn as_tizen(&self) -> Option<&TizenClient> {
        match self {
            Self::Tizen { client, .. } => Some(client),
            _ => None,
        }
    }

    /// The webOS client, when this handle targets an LG TV.
    pub fn as_webos(&self) -> Option<&WebosClient> {
        match self {
            Self::Webos { client, .. } => Some(client),
            _ => None,
        }
    }

    /// The Roku client, when this handle targets a Roku device.
    pub fn as_roku(&self) -> Option<&RokuClient> {
        match self {
            Self::Roku { client, .. } => Some(client),
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
    /// Returns [`TvError`]; the vendor error is mapped in place, each arm naming
    /// [`Capability::PowerState`]. Every mapping lives in [`map`].
    pub async fn power_off(&self) -> Result<()> {
        match self {
            Self::Tizen { client, pool, .. } => pool
                .send_key(client, TizenRemoteKey::PowerOff)
                .await
                .map_err(|error| map::tizen(&error, Capability::PowerState)),
            Self::Webos { client, pool, .. } => pool
                .power_off(client)
                .await
                .map_err(|error| map::webos(&error, Capability::PowerState)),
            Self::Roku { client, .. } => {
                client
                    .power_off()
                    .await
                    .map_err(|error| map::roku(&error, Capability::PowerState))?;
                Ok(())
            }
        }
    }

    /// Ask the TV for its power state.
    ///
    /// Roku answers authoritatively from `/query/device-info`. Tizen and webOS
    /// have no query that still answers once the panel is off, so the shared TCP
    /// probe runs against the control port instead: a completed connect means `On`,
    /// a refused one means `Standby` (network stack up, apps down), and a timeout
    /// means `Off`.
    ///
    /// # Errors
    ///
    /// Returns [`TvError`]; the vendor error is mapped in place. A Roku that is
    /// genuinely off fails the HTTP request rather than answering "off", so a
    /// transport failure there is reported, not converted.
    pub async fn power_state(&self) -> Result<PowerState> {
        // One implementation: the cheap answer is a projection of the careful one, so
        // the two can never disagree about a device.
        Ok(self
            .power_observation()
            .await?
            .value
            .unwrap_or(PowerState::Unknown))
    }

    /// Like [`TvClient::power_state`], but says how sure the answer is.
    ///
    /// Roku reports its power mode over ECP, so the answer is `High`. Tizen and
    /// webOS are probed over TCP: an open control port proves the panel is powered,
    /// and a closed one is good evidence it is not, but neither means a picture is
    /// on the screen (a panel with *Always Ready* keeps the port open while dark).
    /// That is an inference, and reporting it as `High` is how a fleet starts lying
    /// about what is actually showing.
    ///
    /// # Errors
    ///
    /// Returns [`TvError`]; the Roku error maps through [`map::roku`], the TCP
    /// probe's through [`map::power`]. A refused probe is not an error — it is
    /// `Off`.
    pub async fn power_observation(&self) -> Result<ObservedField<PowerState>> {
        match self {
            Self::Roku { client, .. } => Ok(ObservedField::from_read(map_roku_power(
                client
                    .power_state()
                    .await
                    .map_err(|error| map::roku(&error, Capability::PowerState))?,
            ))),
            Self::Tizen { client, .. } => {
                let probe = format!("{}:{}", client.host(), client.port());
                Ok(ObservedField::from_inference(
                    tv_rs_power::poll_power_state(&probe)
                        .await
                        .map_err(|error| map::power(&error))?,
                ))
            }
            Self::Webos { client, .. } => {
                let probe = format!("{}:{}", client.host(), client.port());
                Ok(ObservedField::from_inference(
                    tv_rs_power::poll_power_state(&probe)
                        .await
                        .map_err(|error| map::power(&error))?,
                ))
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
    /// Returns [`TvError`]; the vendor error is mapped in place, naming
    /// [`Capability::AppLaunch`].
    pub async fn launch_app(&self, app_id: &str) -> Result<()> {
        match self {
            Self::Tizen { client, .. } => {
                client
                    .launch_app_via_dial(app_id, None)
                    .await
                    .map_err(|error| map::tizen(&error, Capability::AppLaunch))?;
                Ok(())
            }
            Self::Webos { client, pool, .. } => pool
                .launch_app(client, app_id)
                .await
                .map_err(|error| map::webos(&error, Capability::AppLaunch)),
            Self::Roku { client, .. } => {
                client
                    .launch_app(app_id)
                    .await
                    .map_err(|error| map::roku(&error, Capability::AppLaunch))?;
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "quirks")]
    #[test]
    fn a_quirk_row_pins_the_port_the_panel_actually_uses() {
        // QN90 + T-KTM firmware: 8001 is dead, so the ladder must not probe it.
        let mut tv = DiscoveredTv {
            vendor: TvVendor::Samsung,
            ip: "192.168.1.10".to_string(),
            location: None,
            friendly_name: None,
            manufacturer: Some("Samsung".to_string()),
            model_name: Some("QN90A".to_string()),
            serial_number: None,
            usn: None,
        };
        let client = TvClient::from_discovered(&tv).expect("samsung is supported");
        match &client {
            TvClient::Tizen { client: tizen, .. } => {
                assert_eq!(
                    tizen.pinned_port_number(),
                    Some(8002),
                    "the quirk row must remove 8001 from the ladder"
                );
            }
            other => panic!("expected a Tizen client, got {other:?}"),
        }

        // A model the table says nothing about keeps the full ladder.
        tv.model_name = Some("UE40".to_string());
        let client = TvClient::from_discovered(&tv).expect("samsung is supported");
        match &client {
            TvClient::Tizen { client: tizen, .. } => assert_eq!(tizen.pinned_port_number(), None),
            other => panic!("expected a Tizen client, got {other:?}"),
        }
    }

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
        assert!(matches!(error, TvError::UnsupportedDevice { .. }));
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

/// Maps a Roku power mode onto the shared power vocabulary.
///
/// `Headless` and `Ready` are both "the box is awake": a Roku in either state will
/// accept a keypress, so reporting them as `Off` would make a working device look
/// dead. `DisplayOff` is the panel off with the device running, which is what
/// `Standby` means for the other vendors.
fn map_roku_power(mode: RokuPowerMode) -> PowerState {
    match mode {
        RokuPowerMode::PowerOn => PowerState::On,
        RokuPowerMode::Headless | RokuPowerMode::Ready | RokuPowerMode::DisplayOff => {
            PowerState::Standby
        }
        RokuPowerMode::Unknown => PowerState::Unknown,
    }
}

#[cfg(test)]
mod power_state_tests {
    use super::map_roku_power;
    use tv_rs_power::PowerState;
    use tv_rs_roku::RokuPowerMode;

    #[test]
    fn power_on_is_on() {
        assert_eq!(map_roku_power(RokuPowerMode::PowerOn), PowerState::On);
    }

    #[test]
    fn awake_states_are_standby_not_off() {
        // A Roku in Headless/Ready still accepts keypresses; calling it Off would
        // make the fleet think the display is unreachable.
        assert_eq!(map_roku_power(RokuPowerMode::Headless), PowerState::Standby);
        assert_eq!(map_roku_power(RokuPowerMode::Ready), PowerState::Standby);
        assert_eq!(
            map_roku_power(RokuPowerMode::DisplayOff),
            PowerState::Standby
        );
    }

    #[test]
    fn unknown_stays_unknown() {
        assert_eq!(map_roku_power(RokuPowerMode::Unknown), PowerState::Unknown);
    }
}
