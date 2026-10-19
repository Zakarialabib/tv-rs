//! Samsung-specific SSDP discovery for the remote-control receiver.
//!
//! A generic SSDP sweep finds every UPnP device on the LAN and then has to guess
//! which of them are controllable televisions. Samsung TVs answer a dedicated
//! search target — `urn:samsung-com:device:RemoteControlReceiver:1` — and nothing
//! else does, so querying that target turns "identify the box" into a lookup.
//!
//! The description fetch still runs, because the ST reply alone carries no name
//! or model, and a device can answer the target without being a TV (a Blu-ray
//! player or soundbar on the same firmware).

use std::collections::BTreeMap;
use std::time::Duration;

use tokio::net::UdpSocket;

use crate::error::{Result, TizenError};

/// Samsung's remote-control receiver search target.
pub const SAMSUNG_REMOTE_ST: &str = "urn:samsung-com:device:RemoteControlReceiver:1";

/// IPv4 SSDP multicast group.
pub const SSDP_MULTICAST_ADDR: &str = "239.255.255.250:1900";

/// A Samsung device found by [`discover`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SamsungTv {
    /// Address the reply came from.
    pub ip: String,
    /// `LOCATION` — the UPnP description document.
    pub location: String,
    /// `<friendlyName>`, when the description exposes it.
    pub name: Option<String>,
    /// `<modelName>`, when the description exposes it.
    pub model: Option<String>,
    /// `<manufacturer>`, when the description exposes it.
    pub manufacturer: Option<String>,
    /// `<UDN>` — the device's stable identity.
    pub udn: Option<String>,
}

impl SamsungTv {
    /// Best available label: name, then model, then IP. Never empty.
    pub fn label(&self) -> &str {
        self.name
            .as_deref()
            .or(self.model.as_deref())
            .unwrap_or(self.ip.as_str())
    }
}

/// Builds the `M-SEARCH` datagram for Samsung's receiver target.
pub fn build_msearch(mx: u8) -> String {
    format!(
        "M-SEARCH * HTTP/1.1\r\n\
         HOST: 239.255.255.250:1900\r\n\
         MAN: \"ssdp:discover\"\r\n\
         MX: {mx}\r\n\
         ST: {SAMSUNG_REMOTE_ST}\r\n\r\n"
    )
}

/// Extracts `(LOCATION, ST)` from one SSDP reply.
///
/// Header keys are matched case-insensitively and each line is split once, so a
/// URL keeps its own colons.
pub fn parse_ssdp_reply(message: &str) -> (Option<String>, Option<String>) {
    let mut location = None;
    let mut st = None;

    for line in message.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        let key = key.trim();
        if key.eq_ignore_ascii_case("LOCATION") {
            location = Some(value.to_string());
        } else if key.eq_ignore_ascii_case("ST") {
            st = Some(value.to_string());
        }
    }

    (location, st)
}

/// True when a device's `ST` or description identifies it as Samsung.
pub fn is_samsung(st: Option<&str>, manufacturer: Option<&str>, model: Option<&str>) -> bool {
    if let Some(st) = st {
        if st.eq_ignore_ascii_case(SAMSUNG_REMOTE_ST) {
            return true;
        }
    }
    let blob = format!(
        "{} {}",
        manufacturer.unwrap_or_default(),
        model.unwrap_or_default()
    )
    .to_ascii_lowercase();
    blob.contains("samsung")
}

/// Discovers Samsung TVs on the LAN within `timeout`.
///
/// Returns an empty vector when nothing answers. A multicast/bind failure is an
/// error ([`TizenError::Protocol`]) because on a host where SSDP cannot run at
/// all, "no TVs found" would be a misleading result.
///
/// ```no_run
/// # async fn run() -> tv_rs_tizen::Result<()> {
/// let tvs = tv_rs_tizen::discover_samsung(std::time::Duration::from_secs(3)).await?;
/// for tv in tvs {
///     println!("{} at {}", tv.label(), tv.ip);
/// }
/// # Ok(()) }
/// ```
pub async fn discover(timeout: Duration) -> Result<Vec<SamsungTv>> {
    let socket = UdpSocket::bind("0.0.0.0:0")
        .await
        .map_err(|error| TizenError::Protocol(format!("cannot bind an SSDP socket: {error}")))?;
    socket
        .set_broadcast(true)
        .map_err(|error| TizenError::Protocol(format!("cannot enable broadcast: {error}")))?;

    let multicast: std::net::SocketAddr = SSDP_MULTICAST_ADDR
        .parse()
        .map_err(|error| TizenError::Protocol(format!("bad SSDP address: {error}")))?;
    socket
        .send_to(build_msearch(3).as_bytes(), multicast)
        .await
        .map_err(|error| TizenError::Protocol(format!("cannot send M-SEARCH: {error}")))?;

    // Keyed by LOCATION so a TV that answers several times is fetched once.
    let mut found: BTreeMap<String, Option<String>> = BTreeMap::new();
    let mut buffer = vec![0_u8; 4096];
    let deadline = tokio::time::Instant::now() + timeout;

    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        match tokio::time::timeout(remaining, socket.recv_from(&mut buffer)).await {
            Ok(Ok((len, _peer))) => {
                let text = String::from_utf8_lossy(&buffer[..len]);
                let (location, st) = parse_ssdp_reply(&text);
                if let Some(location) = location {
                    found.entry(location).or_insert(st);
                }
            }
            // Timeout or socket error: end the collection window.
            _ => break,
        }
    }

    let mut devices = Vec::new();
    for (location, st) in found {
        match describe(&location, st.as_deref()).await {
            Ok(Some(tv)) => devices.push(tv),
            Ok(None) => {}
            Err(_) => {}
        }
    }
    Ok(devices)
}

/// Fetches and parses one description document.
///
/// Returns `Ok(None)` when the device answered the probe but does not identify as
/// Samsung; fetch failures are swallowed per device so one slow device cannot
/// fail the sweep.
async fn describe(location: &str, st: Option<&str>) -> Result<Option<SamsungTv>> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;
    let body = client.get(location).send().await?.text().await?;

    let identity = parse_description(&body)?;
    if !is_samsung(
        st,
        identity.manufacturer.as_deref(),
        identity.model.as_deref(),
    ) {
        return Ok(None);
    }

    let ip = host_of(location).unwrap_or_default();
    Ok(Some(SamsungTv {
        ip,
        location: location.to_string(),
        name: identity.name,
        model: identity.model,
        manufacturer: identity.manufacturer,
        udn: identity.udn,
    }))
}

/// Identity fields read from a UPnP description document.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DescriptionIdentity {
    /// `<friendlyName>`.
    pub name: Option<String>,
    /// `<modelName>`.
    pub model: Option<String>,
    /// `<manufacturer>`.
    pub manufacturer: Option<String>,
    /// `<UDN>` — the device's stable identity.
    pub udn: Option<String>,
}

/// Extracts the identity fields this crate cares about from a UPnP description.
pub fn parse_description(xml: &str) -> Result<DescriptionIdentity> {
    let document = roxmltree::Document::parse(xml)?;
    let mut identity = DescriptionIdentity::default();

    for node in document.descendants() {
        match node.tag_name().name() {
            "friendlyName" => identity.name = node.text().map(str::to_string),
            "modelName" => identity.model = node.text().map(str::to_string),
            "manufacturer" => identity.manufacturer = node.text().map(str::to_string),
            "UDN" => identity.udn = node.text().map(str::to_string),
            _ => {}
        }
    }
    Ok(identity)
}

/// `http://192.168.1.10:8001/...` → `192.168.1.10`.
fn host_of(location: &str) -> Option<String> {
    let rest = location
        .strip_prefix("http://")
        .or_else(|| location.strip_prefix("https://"))?;
    let authority = rest.split('/').next()?;
    let host = authority
        .rsplit_once(':')
        .map_or(authority, |(host, _)| host);
    (!host.is_empty()).then(|| host.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn msearch_targets_the_samsung_receiver() {
        let probe = build_msearch(3);
        assert!(probe.starts_with("M-SEARCH * HTTP/1.1\r\n"));
        assert!(probe.contains("ST: urn:samsung-com:device:RemoteControlReceiver:1"));
        assert!(probe.contains("MX: 3"));
        assert!(probe.ends_with("\r\n\r\n"));
    }

    #[test]
    fn reply_parsing_is_case_insensitive_and_keeps_url_colons() {
        let reply = "HTTP/1.1 200 OK\r\n\
                     location: http://192.168.1.10:9197/dmr\r\n\
                     St: urn:samsung-com:device:RemoteControlReceiver:1\r\n";
        let (location, st) = parse_ssdp_reply(reply);
        assert_eq!(location.as_deref(), Some("http://192.168.1.10:9197/dmr"));
        assert_eq!(st.as_deref(), Some(SAMSUNG_REMOTE_ST));
    }

    #[test]
    fn reply_without_location_yields_none() {
        let (location, _st) = parse_ssdp_reply("HTTP/1.1 200 OK\r\nSERVER: Linux\r\n");
        assert!(location.is_none());
    }

    #[test]
    fn identity_accepts_the_search_target_or_samsung_text() {
        assert!(is_samsung(Some(SAMSUNG_REMOTE_ST), None, None));
        assert!(is_samsung(None, Some("Samsung Electronics"), None));
        assert!(is_samsung(None, Some("Other"), Some("Samsung UE55")));
        assert!(!is_samsung(None, Some("Sony"), Some("BRAVIA")));
    }

    #[test]
    fn description_parser_reads_identity_fields() {
        let xml = r#"<?xml version="1.0"?>
            <root xmlns="urn:schemas-upnp-org:device-1-0">
              <device>
                <friendlyName>[TV] Living Room</friendlyName>
                <manufacturer>Samsung Electronics</manufacturer>
                <modelName>UE55TU8000</modelName>
                <UDN>uuid:1234</UDN>
              </device>
            </root>"#;
        let identity = parse_description(xml).expect("valid xml");
        assert_eq!(identity.name.as_deref(), Some("[TV] Living Room"));
        assert_eq!(identity.model.as_deref(), Some("UE55TU8000"));
        assert_eq!(
            identity.manufacturer.as_deref(),
            Some("Samsung Electronics")
        );
        assert_eq!(identity.udn.as_deref(), Some("uuid:1234"));
    }

    #[test]
    fn description_parser_rejects_malformed_xml() {
        assert!(parse_description("<device><friendlyName>x</device>").is_err());
    }

    #[test]
    fn host_is_extracted_from_location() {
        assert_eq!(
            host_of("http://192.168.1.10:8001/api/v2/"),
            Some("192.168.1.10".to_string())
        );
        assert_eq!(
            host_of("http://10.0.0.2/desc.xml"),
            Some("10.0.0.2".to_string())
        );
        assert_eq!(host_of("ftp://10.0.0.2/x"), None);
    }

    #[test]
    fn label_prefers_name_then_model_then_ip() {
        let mut tv = SamsungTv {
            ip: "10.0.0.3".into(),
            location: String::new(),
            name: Some("Living Room".into()),
            model: Some("UE55".into()),
            manufacturer: None,
            udn: None,
        };
        assert_eq!(tv.label(), "Living Room");
        tv.name = None;
        assert_eq!(tv.label(), "UE55");
        tv.model = None;
        assert_eq!(tv.label(), "10.0.0.3");
    }
}
