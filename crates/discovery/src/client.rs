//! Pure SSDP helpers plus the async [`SsdpScanner`].

use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use tokio::net::UdpSocket;

use crate::error::{DiscoveryError, Result};
use crate::payload::{SsdpDeviceDescriptor, SsdpMsearchResponse};
use crate::protocol::{DiscoveryDiagnostics, DiscoveryOutcome};
use crate::types::{SSDP_ALL_DEVICES, SSDP_MULTICAST_IP, SSDP_UDP_PORT};

/// Default time to wait for SSDP replies.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(2);

/// Maximum size of a single SSDP datagram we are willing to read.
const RECV_BUFFER_SIZE: usize = 8192;

/// The default `MX` value advertised when discovering every device.
pub const DEFAULT_MX: u8 = 2;

/// Build the raw SSDP `M-SEARCH` datagram for `target`.
///
/// The returned string is the exact payload to send to
/// `SSDP_MULTICAST_IP:SSDP_UDP_PORT`, including the trailing blank line.
pub fn build_msearch(target: &str, mx: u8) -> String {
    format!(
        "M-SEARCH * HTTP/1.1\r\n\
         HOST: {SSDP_MULTICAST_IP}:{SSDP_UDP_PORT}\r\n\
         MAN: \"ssdp:discover\"\r\n\
         MX: {mx}\r\n\
         ST: {target}\r\n\
         \r\n"
    )
}

/// Parse a raw SSDP response datagram into an [`SsdpMsearchResponse`].
///
/// Header names are matched case-insensitively; the status line and blank
/// lines are ignored. `source` is the IP address the datagram came from.
pub fn parse_msearch_response(raw: &str, source: &str) -> Result<SsdpMsearchResponse> {
    let mut response = SsdpMsearchResponse {
        source: source.to_string(),
        ..SsdpMsearchResponse::default()
    };

    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((name, value)) = line.split_once(':') else {
            // The response status line (`HTTP/1.1 200 OK`) has no colon.
            continue;
        };
        let value = value.trim().to_string();
        if value.is_empty() {
            continue;
        }
        match name.trim().to_ascii_uppercase().as_str() {
            "LOCATION" => response.location = Some(value),
            "SERVER" => response.server = Some(value),
            "USN" => response.usn = Some(value),
            "ST" => response.st = Some(value),
            "CACHE-CONTROL" => response.cache_control = Some(value),
            _ => {}
        }
    }

    Ok(response)
}

/// Parse a UPnP device descriptor XML document into an [`SsdpDeviceDescriptor`].
///
/// The function walks to the first `<device>` element and reads its direct
/// children. Namespaces are ignored, so descriptors from any vendor work.
pub fn parse_device_descriptor(xml: &str) -> Result<SsdpDeviceDescriptor> {
    let document =
        roxmltree::Document::parse(xml).map_err(|err| DiscoveryError::Xml(err.to_string()))?;

    let device = document
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "device")
        .ok_or_else(|| DiscoveryError::Malformed("missing <device> element".to_string()))?;

    let text = |name: &str| -> Option<String> {
        device
            .children()
            .find(|child| child.is_element() && child.tag_name().name() == name)
            .and_then(|child| child.text())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    };

    Ok(SsdpDeviceDescriptor {
        friendly_name: text("friendlyName"),
        manufacturer: text("manufacturer"),
        model_name: text("modelName"),
        model_number: text("modelNumber"),
        udn: text("UDN"),
        device_type: text("deviceType"),
        serial_number: text("serialNumber"),
    })
}

/// Configurable SSDP scanner.
#[derive(Debug, Clone)]
pub struct SsdpScanner {
    timeout: Duration,
    interface: Ipv4Addr,
    bind_addr: SocketAddr,
}

impl Default for SsdpScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl SsdpScanner {
    /// Create a scanner with the defaults: bind `0.0.0.0:0`, interface
    /// `0.0.0.0` and a 2 second timeout.
    pub fn new() -> Self {
        Self {
            timeout: DEFAULT_TIMEOUT,
            interface: Ipv4Addr::UNSPECIFIED,
            bind_addr: SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0)),
        }
    }

    /// Override how long the scanner waits for replies.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Override the local interface used to join the SSDP multicast group.
    pub fn with_interface(mut self, interface: Ipv4Addr) -> Self {
        self.interface = interface;
        self
    }

    /// Override the local address the UDP socket binds to.
    pub fn with_bind_addr(mut self, bind_addr: SocketAddr) -> Self {
        self.bind_addr = bind_addr;
        self
    }

    /// The reply-collection timeout currently configured.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Send an SSDP `M-SEARCH` for `target` and collect the replies, with
    /// answer-window diagnostics.
    ///
    /// The C1 signal lives here: `total_datagrams_received == 0` means the
    /// subnet was silent for the whole `M-SEARCH` window — on a mobile device
    /// whose OS drops multicast, that is the fact that distinguishes "no
    /// displays" from "not allowed to hear them". A zero-datagram window
    /// returns [`DiscoveryOutcome::MulticastSilence`] and only an IO failure
    /// is an error.
    ///
    /// Replies are deduplicated by `USN`, falling back to `LOCATION` when no
    /// `USN` is present.
    pub async fn msearch_outcome(&self, target: &str, mx: u8) -> Result<DiscoveryOutcome> {
        let multicast_addr: Ipv4Addr = SSDP_MULTICAST_IP
            .parse()
            .map_err(|err: std::net::AddrParseError| DiscoveryError::Malformed(err.to_string()))?;

        let socket = UdpSocket::bind(self.bind_addr).await?;
        socket.join_multicast_v4(multicast_addr, self.interface)?;

        let datagram = build_msearch(target, mx);
        socket
            .send_to(datagram.as_bytes(), (multicast_addr, SSDP_UDP_PORT))
            .await?;

        let mut responses: Vec<SsdpMsearchResponse> = Vec::new();
        let mut datagrams = 0usize;
        let mut unique_usns: Vec<String> = Vec::new();
        let mut seen: Vec<String> = Vec::new();
        let mut buffer = vec![0u8; RECV_BUFFER_SIZE];
        let deadline = tokio::time::Instant::now() + self.timeout;

        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                break;
            }

            match tokio::time::timeout(remaining, socket.recv_from(&mut buffer)).await {
                Ok(Ok((len, peer))) => {
                    datagrams += 1;
                    let raw = String::from_utf8_lossy(&buffer[..len]);
                    let response = parse_msearch_response(&raw, &peer.ip().to_string())?;

                    if let Some(usn) = response.usn.as_deref() {
                        if !unique_usns.iter().any(|seen| seen == usn) {
                            unique_usns.push(usn.to_owned());
                        }
                    }

                    if let Some(key) = response.usn.clone().or_else(|| response.location.clone()) {
                        if seen.contains(&key) {
                            continue;
                        }
                        seen.push(key);
                    }

                    responses.push(response);
                }
                Ok(Err(err)) => return Err(DiscoveryError::Io(err)),
                Err(_) => break,
            }
        }

        let diagnostics = DiscoveryDiagnostics {
            total_datagrams_received: datagrams,
            unique_usns: unique_usns.len(),
        };

        if datagrams == 0 {
            // Nothing answered at all. An empty list would read as "no displays
            // on this network", which is a different statement and usually the
            // wrong one.
            return Ok(DiscoveryOutcome::MulticastSilence(diagnostics));
        }

        Ok(DiscoveryOutcome::Found {
            responses,
            diagnostics,
        })
    }

    /// Send an SSDP `M-SEARCH` for `target` and collect the replies.
    ///
    /// Legacy-shaped sugar over [`msearch_outcome`](Self::msearch_outcome): a
    /// silent window stays an error here
    /// ([`DiscoveryError::SilentProbe`]), so existing callers keep their
    /// contract. Callers that want the C1 distinction call `msearch_outcome`.
    pub async fn msearch(&self, target: &str, mx: u8) -> Result<Vec<SsdpMsearchResponse>> {
        match self.msearch_outcome(target, mx).await? {
            DiscoveryOutcome::Found { responses, .. } => Ok(responses),
            DiscoveryOutcome::MulticastSilence(_) => Err(DiscoveryError::SilentProbe {
                target: target.to_string(),
            }),
        }
    }

    /// Discover every SSDP device on the local network.
    pub async fn discover(&self) -> Result<Vec<SsdpMsearchResponse>> {
        self.msearch(SSDP_ALL_DEVICES, DEFAULT_MX).await
    }

    /// Fetch and parse the UPnP device descriptor at `location`.
    pub async fn fetch_descriptor(&self, location: &str) -> Result<SsdpDeviceDescriptor> {
        let body = reqwest::get(location).await?.text().await?;
        parse_device_descriptor(&body)
    }

    /// Discover devices, then fetch each device descriptor.
    ///
    /// Entries without a `LOCATION` header, or whose descriptor fetch fails,
    /// are skipped silently.
    pub async fn discover_tvs(&self) -> Result<Vec<(SsdpMsearchResponse, SsdpDeviceDescriptor)>> {
        self.discover_tvs_with(|_, _| {}).await
    }

    /// Like [`discover_tvs`](Self::discover_tvs), calling `on_found` as each
    /// device's descriptor resolves.
    ///
    /// The callback is the point: a television that answers in 40 ms should
    /// surface then, not after the slowest set in the reply window has answered.
    /// A caller that wants the whole list anyway gets it as the return value.
    ///
    /// The socket loop is written out rather than shared with
    /// [`msearch`](Self::msearch) because the descriptor fetch has to `await`
    /// inside it; a per-reply async hook would need boxed futures to buy the same
    /// thing. Entries without a `LOCATION`, or whose fetch fails, are skipped
    /// silently — the same rule as `discover_tvs`.
    ///
    /// # Errors
    ///
    /// Same taxonomy as [`msearch`](Self::msearch), including
    /// [`DiscoveryError::SilentProbe`] when nothing answers at all.
    pub async fn discover_tvs_with<F>(
        &self,
        mut on_found: F,
    ) -> Result<Vec<(SsdpMsearchResponse, SsdpDeviceDescriptor)>>
    where
        F: FnMut(&SsdpMsearchResponse, &SsdpDeviceDescriptor),
    {
        let multicast_addr: Ipv4Addr = SSDP_MULTICAST_IP
            .parse()
            .map_err(|err: std::net::AddrParseError| DiscoveryError::Malformed(err.to_string()))?;

        let socket = UdpSocket::bind(self.bind_addr).await?;
        socket.join_multicast_v4(multicast_addr, self.interface)?;

        let datagram = build_msearch(SSDP_ALL_DEVICES, DEFAULT_MX);
        socket
            .send_to(datagram.as_bytes(), (multicast_addr, SSDP_UDP_PORT))
            .await?;

        let mut discovered: Vec<(SsdpMsearchResponse, SsdpDeviceDescriptor)> = Vec::new();
        let mut datagrams = 0usize;
        let mut seen: Vec<String> = Vec::new();
        let mut buffer = vec![0u8; RECV_BUFFER_SIZE];
        let deadline = tokio::time::Instant::now() + self.timeout;

        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                break;
            }

            match tokio::time::timeout(remaining, socket.recv_from(&mut buffer)).await {
                Ok(Ok((len, peer))) => {
                    datagrams += 1;
                    let raw = String::from_utf8_lossy(&buffer[..len]);
                    let response = parse_msearch_response(&raw, &peer.ip().to_string())?;

                    if let Some(key) = response.usn.clone().or_else(|| response.location.clone()) {
                        if seen.contains(&key) {
                            continue;
                        }
                        seen.push(key);
                    }

                    let Some(location) = response.location.clone() else {
                        continue;
                    };
                    let Ok(descriptor) = self.fetch_descriptor(&location).await else {
                        continue;
                    };
                    on_found(&response, &descriptor);
                    discovered.push((response, descriptor));
                }
                Ok(Err(err)) => return Err(DiscoveryError::Io(err)),
                Err(_) => break,
            }
        }

        if datagrams == 0 {
            // Nothing answered. An empty list would read as "no displays on this
            // network", which is a different statement and usually the wrong one.
            return Err(DiscoveryError::SilentProbe {
                target: SSDP_ALL_DEVICES.to_string(),
            });
        }

        Ok(discovered)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RESPONSE_UPPER: &str = "HTTP/1.1 200 OK\r\n\
        CACHE-CONTROL: max-age=1800\r\n\
        EXT:\r\n\
        LOCATION: http://192.168.1.10:9197/dmr\r\n\
        SERVER: Linux/2.6.18 UPnP/1.0 Samsung UPnP SDK/1.0\r\n\
        ST: urn:schemas-upnp-org:device:MediaRenderer:1\r\n\
        USN: uuid:9abcd123-4567-89ab-cdef-0123456789ab::urn:schemas-upnp-org:device:MediaRenderer:1\r\n\
        \r\n";

    const RESPONSE_LOWER: &str = "HTTP/1.1 200 OK\r\n\
        cache-control: max-age=1800\r\n\
        location: http://192.168.1.20:1900/desc.xml\r\n\
        server: WebOS/1.0 UPnP/1.0 LG Electronics\r\n\
        st: upnp:rootdevice\r\n\
        usn: uuid:deadbeef-0000-1111-2222-333344445555::upnp:rootdevice\r\n\
        \r\n";

    const DESCRIPTOR_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<root xmlns="urn:schemas-upnp-org:device-1-0">
  <specVersion>
    <major>1</major>
    <minor>0</minor>
  </specVersion>
  <device>
    <deviceType>urn:schemas-upnp-org:device:MediaRenderer:1</deviceType>
    <friendlyName>[TV] Living Room TV</friendlyName>
    <manufacturer>Samsung Electronics</manufacturer>
    <modelName>UE55TU8000</modelName>
    <modelNumber>UE55TU8000XXC</modelNumber>
    <serialNumber>0A1B2C3D4E5F</serialNumber>
    <UDN>uuid:12345678-1234-1234-1234-1234567890ab</UDN>
    <serviceList>
      <service>
        <serviceType>urn:schemas-upnp-org:service:AVTransport:1</serviceType>
      </service>
    </serviceList>
  </device>
</root>
"#;

    #[test]
    fn build_msearch_contains_required_fields() {
        let datagram = build_msearch("ssdp:all", 2);
        assert!(datagram.contains("M-SEARCH * HTTP/1.1"));
        assert!(datagram.contains("MAN: \"ssdp:discover\""));
        assert!(datagram.contains("MX: 2"));
        assert!(datagram.contains("ST: ssdp:all"));
        assert!(datagram.contains("HOST: 239.255.255.250:1900"));
        assert!(datagram.ends_with("\r\n\r\n"));
    }

    #[test]
    fn parse_msearch_response_uppercase_headers() {
        let response = parse_msearch_response(RESPONSE_UPPER, "192.168.1.10").unwrap();

        assert_eq!(
            response.location.as_deref(),
            Some("http://192.168.1.10:9197/dmr")
        );
        assert_eq!(
            response.server.as_deref(),
            Some("Linux/2.6.18 UPnP/1.0 Samsung UPnP SDK/1.0")
        );
        assert_eq!(
            response.usn.as_deref(),
            Some("uuid:9abcd123-4567-89ab-cdef-0123456789ab::urn:schemas-upnp-org:device:MediaRenderer:1")
        );
        assert_eq!(
            response.st.as_deref(),
            Some("urn:schemas-upnp-org:device:MediaRenderer:1")
        );
        assert_eq!(response.cache_control.as_deref(), Some("max-age=1800"));
        assert_eq!(response.source, "192.168.1.10");
    }

    #[test]
    fn parse_msearch_response_lowercase_headers() {
        let response = parse_msearch_response(RESPONSE_LOWER, "192.168.1.20").unwrap();

        assert_eq!(
            response.location.as_deref(),
            Some("http://192.168.1.20:1900/desc.xml")
        );
        assert_eq!(
            response.server.as_deref(),
            Some("WebOS/1.0 UPnP/1.0 LG Electronics")
        );
        assert_eq!(response.st.as_deref(), Some("upnp:rootdevice"));
        assert_eq!(response.cache_control.as_deref(), Some("max-age=1800"));
        assert_eq!(response.source, "192.168.1.20");
    }

    #[test]
    fn parse_device_descriptor_reads_all_fields() {
        let descriptor = parse_device_descriptor(DESCRIPTOR_XML).unwrap();

        assert_eq!(
            descriptor.friendly_name.as_deref(),
            Some("[TV] Living Room TV")
        );
        assert_eq!(
            descriptor.manufacturer.as_deref(),
            Some("Samsung Electronics")
        );
        assert_eq!(descriptor.model_name.as_deref(), Some("UE55TU8000"));
        assert_eq!(descriptor.model_number.as_deref(), Some("UE55TU8000XXC"));
        assert_eq!(
            descriptor.udn.as_deref(),
            Some("uuid:12345678-1234-1234-1234-1234567890ab")
        );
        assert_eq!(
            descriptor.device_type.as_deref(),
            Some("urn:schemas-upnp-org:device:MediaRenderer:1")
        );
        assert_eq!(descriptor.serial_number.as_deref(), Some("0A1B2C3D4E5F"));
    }

    #[test]
    fn parse_device_descriptor_malformed_returns_xml_error() {
        let error = parse_device_descriptor("<root><device></root>").unwrap_err();
        assert!(matches!(error, DiscoveryError::Xml(_)));
    }

    #[test]
    fn scanner_defaults_and_builders() {
        let scanner = SsdpScanner::new();
        assert_eq!(scanner.timeout(), Duration::from_secs(2));

        let custom = SsdpScanner::new()
            .with_timeout(Duration::from_millis(500))
            .with_interface(Ipv4Addr::new(192, 168, 1, 5))
            .with_bind_addr(SocketAddr::from((Ipv4Addr::new(192, 168, 1, 5), 0)));
        assert_eq!(custom.timeout(), Duration::from_millis(500));
    }
}
