//! Direct unicast probing: first contact with a display the picker already
//! knows an address for, and the bounded sweep that substitutes for a subnet
//! whose multicast is dropped.
//!
//! Two surfaces, one vocabulary:
//!
//! * [`probe_host`] is the `G15` probe — a picker ("the user just typed an IP")
//!   needs to know *what* is at that address before opening a control
//!   channel. It tries the four high-signal vendor ports in order and answers
//!   with a [`ProbedEndpoint`] or a nothing.
//! * [`sweep_subnet_bounded`] is the fallback for the C1 world: multicast
//!   silent, so discovery walks the subnet's unicast instead — bounded to 32
//!   concurrent sockets so a phone's radios and a hotel NAT both survive it,
//!   and behind the `mobile` feature so a library consumer never gets a
//!   host-scanning surface unless they asked for one.
//!
//! A `ConnectionRefused` from a television is a fact about power, not about
//! absence: the TV's network stack answers but its discovery/control daemons
//! are down. `02-playbook.md` §2 calls that state standby, and
//! [`ProbeSignal::Standby`] carries it so a picker can show "found a TV, it is
//! asleep" instead of "found nothing".

use std::net::IpAddr;
use std::time::Duration;

use ipnet::Ipv4Net;
use serde::{Deserialize, Serialize};
use tokio::net::TcpStream;

use crate::error::DiscoveryError;

/// The TCP connect timeout each probe port gets. Chosen for the picker: 250 ms
/// is longer than a LAN round-trip and short enough that a sweep of a /24
/// under 32-way concurrency stays in single-digit seconds.
pub const PROBE_CONNECT_TIMEOUT: Duration = Duration::from_millis(250);

/// Tizen's model/Wi-Fi-MAC endpoint on the websocket port.
pub const TIZEN_API_PORT: u16 = 8001;
/// webOS SSAP's websocket port; a connect is the only signal this crate can
/// afford without speaking SSAP.
pub const WEBOS_SSAP_PORT: u16 = 3000;
/// Roku ECP's `device-info` endpoint.
pub const ROKU_ECP_PORT: u16 = 8060;
/// Android TV Remote Protocol v2's TLS endpoint.
pub const ATR_V2_PORT: u16 = 6467;

/// The swept probe ports, in [`probe_host`]'s priority order.
const PROBE_PORTS: [u16; 4] = [TIZEN_API_PORT, WEBOS_SSAP_PORT, ROKU_ECP_PORT, ATR_V2_PORT];

/// What framework the probed host runs, as far as one port can say.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "vendor", content = "detail", rename_all = "snake_case")]
pub enum ProbedVendor {
    /// Samsung Tizen. Detail: the model name (`device.modelName`) when the
    /// `/api/v2/` document answered.
    Tizen {
        /// Reported model name, when the API gave one.
        model: Option<String>,
        /// Reported Wi-Fi MAC, when the API gave one — the Wake-on-LAN
        /// prerequisite.
        wifi_mac: Option<String>,
    },
    /// LG webOS. Detected from the SSAP port answering a TCP connect.
    WebOs,
    /// Roku OS. Detail: the model number from the `device-info` XML, when the
    /// document answered.
    Roku {
        /// Reported model number, when the XML gave one.
        model: Option<String>,
    },
    /// Android TV Remote Protocol v2. Detected from its distinctive port
    /// completing the TCP connect.
    Atr,
}

impl ProbedVendor {
    /// A stable `snake_case` label for logs.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Tizen { .. } => "tizen",
            Self::WebOs => "webos",
            Self::Roku { .. } => "roku",
            Self::Atr => "atr",
        }
    }
}

/// What the probe concluded about the host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeSignal {
    /// A vendor answered on its port.
    Awake {
        /// Which framework, as far as one port can say.
        vendor: ProbedVendor,
        /// The port that positively identified it.
        port: u16,
    },
    /// Nothing identified itself, but the host **refused** every probe port:
    /// the §2 fact — the machine is powered and on the network, the TV
    /// daemons are not answering. Displayed as "asleep", never as "the host is
    /// gone".
    Standby {
        /// The ports that refused, for the UI's "why do I say standby".
        ports_refused: Vec<u16>,
    },
}

/// One host's answer to a probe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProbedEndpoint {
    /// The host that was probed.
    pub ip: IpAddr,
    /// What it is doing.
    pub signal: ProbeSignal,
}

impl ProbedEndpoint {
    /// Whether the probe positively identified a running framework.
    #[must_use]
    pub fn is_awake(&self) -> bool {
        matches!(self.signal, ProbeSignal::Awake { .. })
    }

    /// The vendor, when the probe got one.
    #[must_use]
    pub fn vendor(&self) -> Option<&ProbedVendor> {
        match &self.signal {
            ProbeSignal::Awake { vendor, .. } => Some(vendor),
            ProbeSignal::Standby { .. } => None,
        }
    }
}

/// Probes one host on the four high-signal TV ports, in priority order.
///
/// The HTTP-speaking ports (Tizen `8001`, Roku `8060`) get a document when the
/// TCP connect succeeds and use it to name the model; the pure-connect ports
/// (`3000` SSAP, `6467` ATR) rely on the port being distinctive. Everything the
/// probe does is bounded, and a host that refuses every port comes back as
/// [`ProbeSignal::Standby`] — the §2 mapping — rather than as a bare `None`.
///
/// # Errors
///
/// Returns [`DiscoveryError::Malformed`] / [`DiscoveryError::Xml`] when a
/// connected port served a document this crate claims to read, and that
/// document failed to decode — an unexpected mid-chat failure, not an absent
/// device. Absence, refusal, timeout and filtering are all ordinary probe
/// results, not errors.
pub async fn probe_host(
    ip: IpAddr,
    timeout: Duration,
) -> Result<Option<ProbedEndpoint>, DiscoveryError> {
    let mut refused: Vec<u16> = Vec::new();

    for port in PROBE_PORTS {
        match tokio::time::timeout(PROBE_CONNECT_TIMEOUT, TcpStream::connect((ip, port))).await {
            Ok(Ok(_stream)) => {
                let vendor = match identify_awake_vendor(ip, port, timeout).await? {
                    Some(vendor) => Some(vendor),
                    // The port answered TCP but its protocol did not: keep the
                    // port-level hint rather than throwing the connect fact
                    // away.
                    None => vendor_from_port(port),
                };
                if let Some(vendor) = vendor {
                    return Ok(Some(ProbedEndpoint {
                        ip,
                        signal: ProbeSignal::Awake { vendor, port },
                    }));
                }
            }
            Ok(Err(err)) if err.kind() == std::io::ErrorKind::ConnectionRefused => {
                refused.push(port);
            }
            // Timed out, or the OS answered some other way (firewall DROP,
            // unreachable route). Neither is a fact worth carrying: the sweep
            // was told "silent", not "standby".
            Ok(Err(_)) | Err(_) => {}
        }
    }

    if refused.is_empty() {
        return Ok(None);
    }

    Ok(Some(ProbedEndpoint {
        ip,
        signal: ProbeSignal::Standby {
            ports_refused: refused,
        },
    }))
}

/// The vendor a port implies, when its protocol could not be spoken.
fn vendor_from_port(port: u16) -> Option<ProbedVendor> {
    match port {
        TIZEN_API_PORT => Some(ProbedVendor::Tizen {
            model: None,
            wifi_mac: None,
        }),
        WEBOS_SSAP_PORT => Some(ProbedVendor::WebOs),
        ROKU_ECP_PORT => Some(ProbedVendor::Roku { model: None }),
        ATR_V2_PORT => Some(ProbedVendor::Atr),
        _ => None,
    }
}

/// Fetches the identifying document the awake port serves, when it serves one.
///
/// `None` means "spoke nothing this crate can read"; the caller keeps the
/// port-level hint.
async fn identify_awake_vendor(
    ip: IpAddr,
    port: u16,
    timeout: Duration,
) -> Result<Option<ProbedVendor>, DiscoveryError> {
    let client = reqwest::Client::builder()
        .timeout(timeout)
        .connect_timeout(timeout)
        .build()
        .map_err(|error| DiscoveryError::Malformed(error.to_string()))?;

    match port {
        TIZEN_API_PORT => {
            let url = format!("http://{ip}:{port}/api/v2/");
            match client.get(&url).send().await {
                Ok(response) => {
                    let text = response.text().await.unwrap_or_default();
                    let parsed = parse_tizen_api(&text)?;
                    Ok(Some(ProbedVendor::Tizen {
                        model: parsed.0,
                        wifi_mac: parsed.1,
                    }))
                }
                // An HTTP failure after a successful TCP connect is not a
                // vendor fact; the connect hint stands.
                Err(_) => Ok(None),
            }
        }
        ROKU_ECP_PORT => {
            let url = format!("http://{ip}:{port}/query/device-info");
            match client.get(&url).send().await {
                Ok(response) => {
                    let text = response.text().await.unwrap_or_default();
                    let parsed = parse_roku_device_info(&text)?;
                    Ok(Some(ProbedVendor::Roku { model: parsed }))
                }
                Err(_) => Ok(None),
            }
        }
        _ => Ok(None),
    }
}

/// Pulls `(model, wifi_mac)` out of a Tizen `/api/v2/` document.
fn parse_tizen_api(json: &str) -> Result<(Option<String>, Option<String>), DiscoveryError> {
    let value: serde_json::Value = serde_json::from_str(json.trim())
        .map_err(|error| DiscoveryError::Malformed(format!("tizen api doc: {error}")))?;
    let device = value.get("device").unwrap_or(&value);
    Ok((
        string_field(device, "modelName"),
        string_field(device, "wifiMac"),
    ))
}

/// Pulls the model number out of a Roku `device-info` XML document.
fn parse_roku_device_info(xml: &str) -> Result<Option<String>, DiscoveryError> {
    if xml.trim().is_empty() {
        return Ok(None);
    }
    let document =
        roxmltree::Document::parse(xml).map_err(|error| DiscoveryError::Xml(error.to_string()))?;
    let model = document
        .root_element()
        .children()
        .find(|node| node.is_element() && node.tag_name().name() == "model-number")
        .and_then(|node| node.text())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    Ok(model.map(str::to_owned))
}

fn string_field(value: &serde_json::Value, field: &str) -> Option<String> {
    value
        .get(field)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

/// Sweeps a subnet's hosts with the direct probe, capped at 32 concurrent
/// sockets.
///
/// This is the fallback for a multicast-silent subnet (the C1 outcome): the
/// same [`probe_host`] fact-finding, walked over every unicast address in
/// `subnet_v4` instead of listening for replies that will not come. The
/// concurrency cap exists because the caller is usually a phone: 32 sockets is
/// inside every mobile NIC's comfort zone and every hotel Wi-Fi's NAT.
///
/// Awake endpoints and standby signals are both kept — the §2 fact that a
/// refused probe means "asleep, not absent" is exactly what a commissioning
/// picker needs on a wall of screensavers.
#[cfg(feature = "mobile")]
pub async fn sweep_subnet_bounded(subnet_v4: Ipv4Net) -> Vec<ProbedEndpoint> {
    const MAX_CONCURRENT_SOCKETS: usize = 32;

    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT_SOCKETS));
    let mut handles: Vec<tokio::task::JoinHandle<Option<ProbedEndpoint>>> = Vec::new();

    // Hosts only: a subnet address, its network id or its broadcast is not a
    // screen, and probing it wastes a socket slot.
    for ip in subnet_v4.hosts() {
        let permit = match semaphore.clone().acquire_owned().await {
            Ok(permit) => permit,
            // The semaphore is never closed; this arm is for symmetry only.
            Err(_) => break,
        };
        handles.push(tokio::spawn(async move {
            let _permit = permit;
            // The sweep uses the connect timeout for everything: a picker on a
            // slow vendor policy file would still rather be early.
            probe_host(IpAddr::V4(ip), PROBE_CONNECT_TIMEOUT)
                .await
                .ok()
                .flatten()
        }));
    }

    let mut results: Vec<ProbedEndpoint> = Vec::new();
    for handle in handles {
        if let Ok(Some(endpoint)) = handle.await {
            results.push(endpoint);
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ports_have_the_probe_priority_order() {
        assert_eq!(
            PROBE_PORTS,
            [TIZEN_API_PORT, WEBOS_SSAP_PORT, ROKU_ECP_PORT, ATR_V2_PORT]
        );
    }

    #[test]
    fn tizen_api_doc_names_model_and_wifi_mac() {
        let document = r#"{"device": {
            "modelName": "UE55TU8000",
            "networkType": "wireless",
            "wifiMac": "aa:bb:cc:dd:ee:ff"
        }}"#;
        let (model, wifi_mac) = parse_tizen_api(document).unwrap();
        assert_eq!(model.as_deref(), Some("UE55TU8000"));
        assert_eq!(wifi_mac.as_deref(), Some("aa:bb:cc:dd:ee:ff"));
    }

    #[test]
    fn tizen_api_doc_with_top_level_fields_still_answers() {
        // Some firmware omits the nesting; "device-less" shapes still parse.
        let document = r#"{"modelName": "LN46C"}"#;
        let (model, wifi_mac) = parse_tizen_api(document).unwrap();
        assert_eq!(model.as_deref(), Some("LN46C"));
        assert_eq!(wifi_mac, None);
    }

    #[test]
    fn a_non_json_body_is_a_malformed_doc_not_a_vendor() {
        assert!(parse_tizen_api("<html>nope</html>").is_err());
    }

    #[test]
    fn roku_device_info_xml_names_the_model() {
        let document = r#"
            <device-info>
                <model-number>4640X</model-number>
                <serial-number>YG002H251356</serial-number>
            </device-info>"#;
        assert_eq!(
            parse_roku_device_info(document).unwrap().as_deref(),
            Some("4640X")
        );
        assert_eq!(parse_roku_device_info("   ").unwrap(), None);
        assert!(parse_roku_device_info("<not-xml").is_err());
    }

    #[test]
    fn vendor_labels_are_stable() {
        for vendor in [
            ProbedVendor::Tizen {
                model: None,
                wifi_mac: None,
            },
            ProbedVendor::WebOs,
            ProbedVendor::Roku { model: None },
            ProbedVendor::Atr,
        ] {
            assert!(!vendor.kind().is_empty());
        }
    }

    #[test]
    fn probed_endpoint_helpers_read_the_signal() {
        let awake = ProbedEndpoint {
            ip: "10.0.0.4".parse().unwrap(),
            signal: ProbeSignal::Awake {
                vendor: ProbedVendor::Roku { model: None },
                port: ROKU_ECP_PORT,
            },
        };
        assert!(awake.is_awake());
        assert_eq!(awake.vendor().unwrap().kind(), "roku");

        let standby = ProbedEndpoint {
            ip: "10.0.0.5".parse().unwrap(),
            signal: ProbeSignal::Standby {
                ports_refused: PROBE_PORTS.to_vec(),
            },
        };
        assert!(!standby.is_awake());
        assert!(standby.vendor().is_none());
    }

    #[cfg(feature = "mobile")]
    #[test]
    fn subnet_enumeration_skips_the_wire_endpoints() {
        // ipnet's Hosts iterator excludes the network address and broadcast,
        // which is why the sweep can use it directly.
        let subnet: Ipv4Net = "10.0.0.0/30".parse().unwrap();
        let hosts: Vec<std::net::Ipv4Addr> = subnet.hosts().collect();
        let expected: Vec<std::net::Ipv4Addr> =
            vec!["10.0.0.1".parse().unwrap(), "10.0.0.2".parse().unwrap()];
        assert_eq!(hosts, expected);
    }
}
