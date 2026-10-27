//! mDNS (Bonjour) service discovery and advertisement for digital signage players.

use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use tokio::net::UdpSocket;

use crate::error::{DiscoveryError, Result};
use crate::types::{MDNS_MULTICAST_IP, MDNS_UDP_PORT};

/// Default service type for tv-rs digital signage players over mDNS.
pub const MDNS_SERVICE_TYPE_SIGNAGE: &str = "_tv-rs-signage._tcp.local";

/// Default service type for tv-rs displays/TVs over mDNS.
pub const MDNS_SERVICE_TYPE_TV: &str = "_tv-rs._tcp.local";

/// Default time to wait for mDNS replies.
const DEFAULT_MDNS_TIMEOUT: Duration = Duration::from_secs(2);

/// Maximum size of a single mDNS UDP datagram.
const RECV_BUFFER_SIZE: usize = 8192;

/// An mDNS service advertisement record representing a signage player or TV on the local network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MdnsServiceRecord {
    /// Service instance name (e.g. `"Signage Player 1._tv-rs-signage._tcp.local"`).
    pub service_name: String,
    /// Service type (e.g. `"_tv-rs-signage._tcp.local"`).
    pub service_type: String,
    /// Hostname (e.g. `"signage-player.local"`).
    pub host_name: String,
    /// Service port number.
    pub port: u16,
    /// Host IPv4 address.
    pub ip: Ipv4Addr,
    /// Key-value TXT metadata records (e.g., `"version"`, `"player_id"`, `"session_token"`).
    pub txt_records: HashMap<String, String>,
}

impl MdnsServiceRecord {
    /// Create a new mDNS service record.
    pub fn new(
        service_name: impl Into<String>,
        service_type: impl Into<String>,
        host_name: impl Into<String>,
        port: u16,
        ip: Ipv4Addr,
    ) -> Self {
        Self {
            service_name: service_name.into(),
            service_type: service_type.into(),
            host_name: host_name.into(),
            port,
            ip,
            txt_records: HashMap::new(),
        }
    }

    /// Add or update a key-value pair in TXT records.
    pub fn with_txt(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.txt_records.insert(key.into(), value.into());
        self
    }
}

/// Encode a domain name into DNS wire format (label sequence terminated by 0).
fn encode_dns_name(name: &str) -> Vec<u8> {
    let mut buf = Vec::new();
    for label in name.split('.').filter(|s| !s.is_empty()) {
        let bytes = label.as_bytes();
        if bytes.len() > 63 {
            continue;
        }
        buf.push(bytes.len() as u8);
        buf.extend_from_slice(bytes);
    }
    buf.push(0);
    buf
}

/// Decode a domain name from DNS wire format, supporting label compression pointers.
fn decode_dns_name(buf: &[u8], mut offset: usize) -> Option<(String, usize)> {
    let mut labels = Vec::new();
    let mut visited = Vec::new();
    let mut next_offset = None;

    loop {
        if offset >= buf.len() || visited.contains(&offset) {
            return None;
        }
        visited.push(offset);

        let len = buf[offset] as usize;
        if len == 0 {
            if next_offset.is_none() {
                next_offset = Some(offset + 1);
            }
            break;
        }

        // Check for DNS pointer compression (bits 11xxxxxx)
        if (len & 0xC0) == 0xC0 {
            if offset + 1 >= buf.len() {
                return None;
            }
            let pointer = ((len & 0x3F) << 8) | (buf[offset + 1] as usize);
            if next_offset.is_none() {
                next_offset = Some(offset + 2);
            }
            offset = pointer;
            continue;
        }

        offset += 1;
        if offset + len > buf.len() {
            return None;
        }

        let label = String::from_utf8_lossy(&buf[offset..offset + len]).to_string();
        labels.push(label);
        offset += len;
    }

    let end_offset = next_offset.unwrap_or(offset);
    let name = labels.join(".");
    Some((name, end_offset))
}

/// Build an mDNS query DNS packet for `service_type` requesting PTR records.
pub fn build_mdns_query(service_type: &str) -> Vec<u8> {
    let mut packet = Vec::new();

    // Transaction ID: 0 for mDNS
    packet.extend_from_slice(&[0x00, 0x00]);
    // Flags: standard query
    packet.extend_from_slice(&[0x00, 0x00]);
    // Questions: 1, Answer RRs: 0, Authority RRs: 0, Additional RRs: 0
    packet.extend_from_slice(&[0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);

    // QNAME
    packet.extend(encode_dns_name(service_type));

    // QTYPE: PTR (12)
    packet.extend_from_slice(&[0x00, 0x0C]);
    // QCLASS: IN (1) + unicast-response bit (0x8000) or standard multicast (1)
    packet.extend_from_slice(&[0x00, 0x01]);

    packet
}

/// Build an mDNS response packet containing PTR, SRV, TXT, and A resource records.
pub fn build_mdns_response(record: &MdnsServiceRecord) -> Vec<u8> {
    let mut packet = Vec::new();

    // Transaction ID: 0
    packet.extend_from_slice(&[0x00, 0x00]);
    // Flags: Response, Authoritative (0x8400)
    packet.extend_from_slice(&[0x84, 0x00]);
    // Questions: 0, Answer RRs: 4 (PTR, SRV, TXT, A), Authority RRs: 0, Additional RRs: 0
    packet.extend_from_slice(&[0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00]);

    let ttl_bytes = 120u32.to_be_bytes();

    // 1. PTR record: service_type -> service_name
    packet.extend(encode_dns_name(&record.service_type));
    packet.extend_from_slice(&[0x00, 0x0C]); // TYPE: PTR
    packet.extend_from_slice(&[0x00, 0x01]); // CLASS: IN
    packet.extend_from_slice(&ttl_bytes);
    let ptr_data = encode_dns_name(&record.service_name);
    packet.extend_from_slice(&(ptr_data.len() as u16).to_be_bytes());
    packet.extend(ptr_data);

    // 2. SRV record: service_name -> priority(0), weight(0), port, host_name
    packet.extend(encode_dns_name(&record.service_name));
    packet.extend_from_slice(&[0x00, 0x21]); // TYPE: SRV
    packet.extend_from_slice(&[0x00, 0x01]); // CLASS: IN
    packet.extend_from_slice(&ttl_bytes);
    let target_dns = encode_dns_name(&record.host_name);
    let srv_len = 6 + target_dns.len();
    packet.extend_from_slice(&(srv_len as u16).to_be_bytes());
    packet.extend_from_slice(&[0x00, 0x00]); // Priority: 0
    packet.extend_from_slice(&[0x00, 0x00]); // Weight: 0
    packet.extend_from_slice(&record.port.to_be_bytes()); // Port
    packet.extend(target_dns);

    // 3. TXT record: service_name -> key=value strings
    packet.extend(encode_dns_name(&record.service_name));
    packet.extend_from_slice(&[0x00, 0x10]); // TYPE: TXT
    packet.extend_from_slice(&[0x00, 0x01]); // CLASS: IN
    packet.extend_from_slice(&ttl_bytes);
    let mut txt_data = Vec::new();
    if record.txt_records.is_empty() {
        txt_data.push(0);
    } else {
        for (k, v) in &record.txt_records {
            let item = format!("{k}={v}");
            let bytes = item.as_bytes();
            if bytes.len() <= 255 {
                txt_data.push(bytes.len() as u8);
                txt_data.extend_from_slice(bytes);
            }
        }
    }
    packet.extend_from_slice(&(txt_data.len() as u16).to_be_bytes());
    packet.extend(txt_data);

    // 4. A record: host_name -> IPv4 address
    packet.extend(encode_dns_name(&record.host_name));
    packet.extend_from_slice(&[0x00, 0x01]); // TYPE: A
    packet.extend_from_slice(&[0x00, 0x01]); // CLASS: IN
    packet.extend_from_slice(&ttl_bytes);
    packet.extend_from_slice(&[0x00, 0x04]); // RDLENGTH: 4
    packet.extend_from_slice(&record.ip.octets());

    packet
}

/// Parse an mDNS packet and extract an [`MdnsServiceRecord`].
pub fn parse_mdns_packet(buf: &[u8]) -> Option<MdnsServiceRecord> {
    if buf.len() < 12 {
        return None;
    }

    let qd_count = u16::from_be_bytes([buf[4], buf[5]]) as usize;
    let an_count = u16::from_be_bytes([buf[6], buf[7]]) as usize;
    let ns_count = u16::from_be_bytes([buf[8], buf[9]]) as usize;
    let ar_count = u16::from_be_bytes([buf[10], buf[11]]) as usize;

    let total_rr = an_count + ns_count + ar_count;
    if total_rr == 0 {
        return None;
    }

    let mut offset = 12;

    // Skip question section
    for _ in 0..qd_count {
        let (_, next_offset) = decode_dns_name(buf, offset)?;
        offset = next_offset + 4; // QTYPE (2) + QCLASS (2)
    }

    let mut service_name = None;
    let mut service_type = None;
    let mut host_name = None;
    let mut port = None;
    let mut ip = None;
    let mut txt_records = HashMap::new();

    // Parse Resource Records
    for _ in 0..total_rr {
        if offset >= buf.len() {
            break;
        }

        let (rr_name, next_offset) = decode_dns_name(buf, offset)?;
        offset = next_offset;

        if offset + 10 > buf.len() {
            break;
        }

        let rr_type = u16::from_be_bytes([buf[offset], buf[offset + 1]]);
        let rd_length = u16::from_be_bytes([buf[offset + 8], buf[offset + 9]]) as usize;
        offset += 10;

        if offset + rd_length > buf.len() {
            break;
        }

        let rdata = &buf[offset..offset + rd_length];
        offset += rd_length;

        match rr_type {
            // A Record
            1 if rd_length == 4 => {
                ip = Some(Ipv4Addr::new(rdata[0], rdata[1], rdata[2], rdata[3]));
                if host_name.is_none() {
                    host_name = Some(rr_name);
                }
            }
            12 => {
                // PTR Record
                if let Some((target, _)) = decode_dns_name(buf, offset - rd_length) {
                    service_type = Some(rr_name);
                    service_name = Some(target);
                }
            }
            16 => {
                // TXT Record
                let mut txt_offset = 0;
                while txt_offset < rdata.len() {
                    let len = rdata[txt_offset] as usize;
                    txt_offset += 1;
                    if txt_offset + len <= rdata.len() && len > 0 {
                        let item = String::from_utf8_lossy(&rdata[txt_offset..txt_offset + len]);
                        if let Some((k, v)) = item.split_once('=') {
                            txt_records.insert(k.to_string(), v.to_string());
                        }
                    }
                    txt_offset += len;
                }
            }
            33 if rd_length >= 7 => {
                // SRV Record: Priority (2), Weight (2), Port (2), Target Name
                let srv_port = u16::from_be_bytes([rdata[4], rdata[5]]);
                port = Some(srv_port);
                if let Some((target, _)) = decode_dns_name(buf, offset - rd_length + 6) {
                    host_name = Some(target);
                }
            }
            _ => {}
        }
    }

    let ip = ip?;
    let port = port.unwrap_or(8443);
    let s_name = service_name.unwrap_or_else(|| "signage-display.local".to_string());
    let s_type = service_type.unwrap_or_else(|| MDNS_SERVICE_TYPE_SIGNAGE.to_string());
    let h_name = host_name.unwrap_or_else(|| "signage-host.local".to_string());

    Some(MdnsServiceRecord {
        service_name: s_name,
        service_type: s_type,
        host_name: h_name,
        port,
        ip,
        txt_records,
    })
}

/// Asynchronous mDNS discovery scanner.
#[derive(Debug, Clone)]
pub struct MdnsScanner {
    timeout: Duration,
    interface: Ipv4Addr,
    bind_addr: SocketAddr,
}

impl Default for MdnsScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl MdnsScanner {
    /// Create an mDNS scanner with default settings.
    pub fn new() -> Self {
        Self {
            timeout: DEFAULT_MDNS_TIMEOUT,
            interface: Ipv4Addr::UNSPECIFIED,
            bind_addr: SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0)),
        }
    }

    /// Override the reply-collection timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Override the local interface used for multicast.
    pub fn with_interface(mut self, interface: Ipv4Addr) -> Self {
        self.interface = interface;
        self
    }

    /// Override the bind address.
    pub fn with_bind_addr(mut self, bind_addr: SocketAddr) -> Self {
        self.bind_addr = bind_addr;
        self
    }

    /// Scan local network for mDNS services matching `service_type`.
    pub async fn discover_services(&self, service_type: &str) -> Result<Vec<MdnsServiceRecord>> {
        let multicast_ip: Ipv4Addr = MDNS_MULTICAST_IP
            .parse()
            .map_err(|e: std::net::AddrParseError| DiscoveryError::Malformed(e.to_string()))?;

        let socket = UdpSocket::bind(self.bind_addr).await?;
        socket.join_multicast_v4(multicast_ip, self.interface)?;

        let query = build_mdns_query(service_type);
        socket
            .send_to(&query, (multicast_ip, MDNS_UDP_PORT))
            .await?;

        let mut services = Vec::new();
        let mut buffer = vec![0u8; RECV_BUFFER_SIZE];
        let deadline = tokio::time::Instant::now() + self.timeout;

        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                break;
            }

            match tokio::time::timeout(remaining, socket.recv_from(&mut buffer)).await {
                Ok(Ok((len, _peer))) => {
                    if let Some(record) = parse_mdns_packet(&buffer[..len]) {
                        if !services.contains(&record) {
                            services.push(record);
                        }
                    }
                }
                Ok(Err(err)) => return Err(DiscoveryError::Io(err)),
                Err(_) => break,
            }
        }

        if services.is_empty() {
            return Err(DiscoveryError::SilentProbe {
                target: service_type.to_string(),
            });
        }

        Ok(services)
    }

    /// Discover tv-rs digital signage players via mDNS.
    pub async fn discover_signage_players(&self) -> Result<Vec<MdnsServiceRecord>> {
        self.discover_services(MDNS_SERVICE_TYPE_SIGNAGE).await
    }
}

/// An mDNS advertiser for digital signage players (Windows/Tauri or embedded hosts).
#[derive(Debug)]
pub struct MdnsAdvertiser {
    record: MdnsServiceRecord,
}

impl MdnsAdvertiser {
    /// Create a new mDNS advertiser for a service record.
    pub fn new(record: MdnsServiceRecord) -> Self {
        Self { record }
    }

    /// Broadcast the service advertisement over mDNS multicast.
    pub async fn advertise_once(&self) -> Result<()> {
        let multicast_ip: Ipv4Addr = MDNS_MULTICAST_IP
            .parse()
            .map_err(|e: std::net::AddrParseError| DiscoveryError::Malformed(e.to_string()))?;

        let socket = UdpSocket::bind(SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0))).await?;
        let packet = build_mdns_response(&self.record);

        socket
            .send_to(&packet, (multicast_ip, MDNS_UDP_PORT))
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_and_decode_dns_name() {
        let name = "player._tv-rs-signage._tcp.local";
        let encoded = encode_dns_name(name);
        let (decoded, len) = decode_dns_name(&encoded, 0).unwrap();
        assert_eq!(decoded, name);
        assert_eq!(len, encoded.len());
    }

    #[test]
    fn test_build_and_parse_mdns_response() {
        let record = MdnsServiceRecord::new(
            "Player1._tv-rs-signage._tcp.local",
            MDNS_SERVICE_TYPE_SIGNAGE,
            "signage1.local",
            8443,
            Ipv4Addr::new(192, 168, 1, 100),
        )
        .with_txt("version", "1.0")
        .with_txt("token", "abc-123");

        let packet = build_mdns_response(&record);
        let parsed = parse_mdns_packet(&packet).expect("Failed to parse built packet");

        assert_eq!(parsed.service_name, record.service_name);
        assert_eq!(parsed.service_type, record.service_type);
        assert_eq!(parsed.host_name, record.host_name);
        assert_eq!(parsed.port, record.port);
        assert_eq!(parsed.ip, record.ip);
        assert_eq!(
            parsed.txt_records.get("version").map(|s| s.as_str()),
            Some("1.0")
        );
        assert_eq!(
            parsed.txt_records.get("token").map(|s| s.as_str()),
            Some("abc-123")
        );
    }

    #[test]
    fn test_build_mdns_query() {
        let query = build_mdns_query(MDNS_SERVICE_TYPE_SIGNAGE);
        assert!(query.len() > 12);
        assert_eq!(query[0..2], [0x00, 0x00]); // ID 0
        assert_eq!(query[4..6], [0x00, 0x01]); // 1 question
    }
}
