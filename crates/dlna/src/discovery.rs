//! SSDP discovery for DLNA/UPnP MediaRenderers.
//!
//! Sends an M-SEARCH to the SSDP multicast group and collects responses for a
//! bounded window, then fetches each device's description XML to populate
//! friendly names. Robust against firewalls: a blocked multicast bind surfaces
//! as [`crate::DlnaError::MulticastBlocked`] rather than panicking.

use std::collections::HashMap;
use std::time::Duration;

use tokio::net::UdpSocket;

use crate::error::DlnaError;
use crate::models::Renderer;

const SSDP_ADDR: &str = "239.255.255.250:1900";
const SSDP_MSEARCH: &str = "M-SEARCH * HTTP/1.1\r\n\
    HOST: 239.255.255.250:1900\r\n\
    MAN: \"ssdp:discover\"\r\n\
    MX: 3\r\n\
    ST: urn:schemas-upnp-org:device:MediaRenderer:1\r\n\
    \r\n";

/// Discover DLNA/UPnP MediaRenderers on the local network.
///
/// `timeout` bounds how long we wait for responses (recommended 3s).
///
/// Sends a single M-SEARCH probe and collects responses. For a more
/// thorough scan that catches devices which missed the first probe,
/// use [`discover_renderers_with_retries`] instead.
pub async fn discover_renderers(timeout: Duration) -> Result<Vec<Renderer>, DlnaError> {
    let socket = match UdpSocket::bind("0.0.0.0:0").await {
        Ok(s) => s,
        Err(_) => return Err(DlnaError::MulticastBlocked),
    };
    if socket.set_broadcast(true).is_err() {
        return Err(DlnaError::MulticastBlocked);
    }

    let multicast: std::net::SocketAddr = SSDP_ADDR.parse().expect("valid SSDP addr");
    if socket.send_to(SSDP_MSEARCH.as_bytes(), multicast).await.is_err() {
        return Err(DlnaError::MulticastBlocked);
    }

    let mut locations: HashMap<String, String> = HashMap::new();
    let mut buf = vec![0u8; 4096];
    let deadline = tokio::time::Instant::now() + timeout;

    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        match tokio::time::timeout(remaining, socket.recv_from(&mut buf)).await {
            Ok(Ok((len, _addr))) => {
                let text = String::from_utf8_lossy(&buf[..len]);
                if let Some(location) = parse_ssdp_location(&text) {
                    if !locations.contains_key(&location) {
                        locations.insert(location.clone(), String::new());
                    }
                }
            }
            _ => break,
        }
    }

    if locations.is_empty() {
        return Err(DlnaError::NoDevicesFound);
    }

    // Fetch device description XML for each unique location to get names.
    let mut renderers = Vec::with_capacity(locations.len());
    for location in locations.keys() {
        match fetch_device_description(location).await {
            Ok(renderer) => renderers.push(renderer),
            Err(e) => log::warn!("[tv-rs-dlna] failed to describe {location}: {e}"),
        }
    }

    if renderers.is_empty() {
        return Err(DlnaError::NoDevicesFound);
    }
    Ok(renderers)
}

/// Discover DLNA MediaRenderers, re-probing with exponential backoff until
/// devices are found or `probes` attempts are exhausted.
///
/// Each probe uses `per_probe_timeout` to bound its response window. When a
/// probe yields nothing, the next one is delayed by
/// `backoff_base_ms * 2^attempt` (milliseconds) so devices that missed the
/// first M-SEARCH have a chance to respond. Results from all probes are
/// combined and deduplicated by UDN.
pub async fn discover_renderers_with_retries(
    probes: u32,
    per_probe_timeout: Duration,
    backoff_base_ms: u64,
) -> Result<Vec<Renderer>, DlnaError> {
    let probes = probes.max(1);
    let mut all: Vec<Renderer> = Vec::new();
    let mut last_err: Option<DlnaError> = None;

    for attempt in 0..probes {
        match discover_renderers(per_probe_timeout).await {
            Ok(renderers) => {
                for renderer in renderers {
                    if !all.iter().any(|d| d.udn == renderer.udn) {
                        all.push(renderer);
                    }
                }
            }
            Err(e) => last_err = Some(e),
        }

        if !all.is_empty() {
            break;
        }

        // Exponential backoff between probes (skip after the final attempt).
        if attempt + 1 < probes {
            let shift: u32 = attempt.min(10);
            let backoff_ms = backoff_base_ms.saturating_mul(1u64 << shift);
            tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
        }
    }

    if all.is_empty() {
        return Err(last_err.unwrap_or(DlnaError::NoDevicesFound));
    }
    Ok(all)
}

/// Extract the `LOCATION` header from an SSDP message.
///
/// Skips lines that are not `Key: Value` rather than bailing out on them. This is
/// essential, not cosmetic: an SSDP response opens with a status line
/// (`HTTP/1.1 200 OK`) that contains no colon. The previous implementation used
/// `line.split_once(':')?` INSIDE the loop, so that very first line made the whole
/// function return `None` — meaning `LOCATION` was never found in any real SSDP
/// reply and discovery silently reported zero devices on every network.
///
/// Note `HTTP/1.1 200 OK` would also have been mis-parsed had it survived: naive
/// splitting on the first colon is unsafe for the *value* too, since a location URL
/// contains `http://…`. Splitting the KEY off once and trimming the remainder keeps
/// the URL intact.
fn parse_ssdp_location(message: &str) -> Option<String> {
    for line in message.lines() {
        // Not a header line (e.g. the leading status line, or a trailing blank) —
        // skip it and keep scanning.
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        if key.trim().eq_ignore_ascii_case("LOCATION") {
            let value = value.trim();
            if value.is_empty() {
                continue;
            }
            return Some(value.to_string());
        }
    }
    None
}

/// Fetch a device's description XML and parse out the renderer identity.
async fn fetch_device_description(location: &str) -> Result<Renderer, DlnaError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;
    let body = client.get(location).send().await?.text().await?;

    let udn = extract_xml_text(&body, "UDN").unwrap_or_else(|| "unknown".to_string());
    let friendly_name = extract_xml_text(&body, "friendlyName").unwrap_or_else(|| udn.clone());
    let manufacturer = extract_xml_text(&body, "manufacturer").unwrap_or_default();
    let model_name = extract_xml_text(&body, "modelName").unwrap_or_default();
    let ip = location
        .split("://")
        .nth(1)
        .and_then(|rest| rest.split(':').next())
        .map(|s| s.to_string())
        .unwrap_or_default();

    // Extract the AVTransport control URL so the device is immediately castable.
    let av_transport_url =
        crate::soap::extract_av_transport_control_url(&body, location).ok();

    // And its eventSubURL, so the caller can SUBSCRIBE for state changes
    // without re-fetching the description. `.ok()` because eventing is optional: a
    // renderer may support control but not events, and that must not block casting.
    let event_sub_url = crate::gena::extract_event_sub_url(&body, location).ok();

    let mut renderer = Renderer::new(
        udn, friendly_name, manufacturer, model_name, ip, location,
    );
    renderer.av_transport_url = av_transport_url;
    renderer.event_sub_url = event_sub_url;
    Ok(renderer)
}

/// Minimal case-insensitive XML element text extraction (avoids a heavy parser
/// dependency for the description doc; SSDP descriptions are small + well-formed).
fn extract_xml_text(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    let value = xml[start..end].trim();
    // Strip a potential CDATA wrapper.
    let value = value
        .trim_start_matches("<![CDATA[")
        .trim_end_matches("]]>");
    Some(value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ssdp_location_header() {
        let msg = "HTTP/1.1 200 OK\r\nCACHE-CONTROL: max-age=1800\r\nLOCATION: http://192.168.1.50:8200/desc.xml\r\nSERVER: Linux/3.14\r\n";
        assert_eq!(
            parse_ssdp_location(msg),
            Some("http://192.168.1.50:8200/desc.xml".to_string())
        );
    }

    #[test]
    fn location_survives_a_leading_status_line() {
        // Regression guard. `parse_ssdp_location` used `split_once(':')?` inside the
        // loop, so the colon-free `HTTP/1.1 200 OK` status line returned None for the
        // WHOLE function — every real SSDP reply starts with one, so discovery found
        // zero devices on every network while the unit test above was the only thing
        // that noticed.
        let msg = "HTTP/1.1 200 OK\r\nLOCATION: http://10.0.0.7:2869/d.xml\r\n";
        assert_eq!(
            parse_ssdp_location(msg),
            Some("http://10.0.0.7:2869/d.xml".to_string())
        );
    }

    #[test]
    fn location_url_keeps_its_own_colons() {
        // The value contains `http://` and a port, so only the KEY may be split off.
        let msg = "NOTIFY * HTTP/1.1\r\nLocation: http://[fe80::1]:8200/desc.xml\r\n";
        assert_eq!(
            parse_ssdp_location(msg),
            Some("http://[fe80::1]:8200/desc.xml".to_string())
        );
    }

    #[test]
    fn header_name_is_case_insensitive() {
        // Real devices send LOCATION, Location and location interchangeably.
        for name in ["LOCATION", "Location", "location"] {
            let msg = format!("HTTP/1.1 200 OK\r\n{name}: http://1.2.3.4/d.xml\r\n");
            assert_eq!(
                parse_ssdp_location(&msg),
                Some("http://1.2.3.4/d.xml".to_string()),
                "{name} must be recognised"
            );
        }
    }

    #[test]
    fn no_location_header_yields_none() {
        let msg = "HTTP/1.1 200 OK\r\nSERVER: Linux/3.14\r\nEXT:\r\n";
        assert_eq!(parse_ssdp_location(msg), None);
    }

    #[test]
    fn empty_location_value_is_not_returned() {
        // An empty LOCATION would otherwise be handed to reqwest as a URL.
        let msg = "HTTP/1.1 200 OK\r\nLOCATION:\r\nSERVER: x\r\n";
        assert_eq!(parse_ssdp_location(msg), None);
    }

    #[test]
    fn extracts_xml_text_case_insensitive() {
        let xml = "<root><device><friendlyName>Living Room TV</friendlyName></device></root>";
        assert_eq!(
            extract_xml_text(xml, "friendlyName"),
            Some("Living Room TV".to_string())
        );
    }

    #[test]
    fn extracts_xml_with_cdata() {
        let xml = "<modelName><![CDATA[OLED55C3]]></modelName>";
        assert_eq!(extract_xml_text(xml, "modelName"), Some("OLED55C3".to_string()));
    }
}
