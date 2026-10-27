//! ADB (Android Debug Bridge) network device scanner.
//!
//! Scans for network-enabled Android TV boxes, Fire TVs, and Amlogic devices
//! listening on TCP port 5555, sending ADB connection initialization frames
//! to extract device model and manufacturer strings.

use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::error::{DiscoveryError, Result};
use crate::fingerprint::DeviceFingerprint;

/// Default TCP port for ADB over IP.
pub const ADB_DEFAULT_PORT: u16 = 5555;

/// Default timeout for probing an ADB network host.
const DEFAULT_ADB_TIMEOUT: Duration = Duration::from_millis(800);

/// An ADB connection message command (`CNXN`).
const ADB_CMD_CNXN: u32 = 0x4E584E43; // "CNXN" in little endian

/// Information extracted from an ADB network device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdbDevice {
    /// Device host IP address.
    pub ip: Ipv4Addr,
    /// ADB TCP port (usually 5555).
    pub port: u16,
    /// Device product model (e.g. `"AFTMM"`, `"SHIELD Android TV"`, `"S905X4"`).
    pub model: Option<String>,
    /// Device manufacturer (e.g. `"Amazon"`, `"NVIDIA"`, `"Amlogic"`).
    pub manufacturer: Option<String>,
    /// Full ADB banner or connection string.
    pub banner: String,
    /// Device fingerprint populated with ADB signature.
    pub fingerprint: DeviceFingerprint,
}

/// Construct an ADB `CNXN` 24-byte packet header + system identity string payload.
pub fn build_adb_cnxn_packet(identity: &str) -> Vec<u8> {
    let payload = identity.as_bytes();
    let data_len = payload.len() as u32;

    // Checksum: sum of all bytes in payload
    let checksum: u32 = payload.iter().map(|&b| b as u32).sum();

    let mut buf = Vec::with_capacity(24 + payload.len());
    buf.extend_from_slice(&ADB_CMD_CNXN.to_le_bytes()); // command
    buf.extend_from_slice(&0x0100_0000u32.to_le_bytes()); // arg0: version (1.0)
    buf.extend_from_slice(&0x0004_0000u32.to_le_bytes()); // arg1: maxdata (256KB)
    buf.extend_from_slice(&data_len.to_le_bytes()); // data_length
    buf.extend_from_slice(&checksum.to_le_bytes()); // data_checksum
    buf.extend_from_slice(&(ADB_CMD_CNXN ^ 0xFFFF_FFFF).to_le_bytes()); // magic
    buf.extend_from_slice(payload);
    buf
}

/// Parse an ADB `CNXN` response banner (e.g. `"device::ro.product.name=...;ro.product.model=AFTMM;..."`).
pub fn parse_adb_banner(banner: &str) -> (Option<String>, Option<String>) {
    let mut model = None;
    let mut manufacturer = None;

    for segment in banner.split(';') {
        let segment = segment.trim();
        if let Some((raw_key, val)) = segment.split_once('=') {
            let key = raw_key
                .rsplit_once("::")
                .map(|(_, k)| k)
                .unwrap_or(raw_key)
                .trim();
            match key {
                "ro.product.model" | "model" => model = Some(val.trim().to_string()),
                "ro.product.manufacturer" | "manufacturer" => {
                    manufacturer = Some(val.trim().to_string());
                }
                "device" if model.is_none() => {
                    model = Some(val.trim().to_string());
                }
                _ => {}
            }
        }
    }

    (model, manufacturer)
}

/// Asynchronous ADB network device scanner.
#[derive(Debug, Clone)]
pub struct AdbScanner {
    port: u16,
    timeout: Duration,
}

impl Default for AdbScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl AdbScanner {
    /// Create a new ADB scanner with default port (5555) and timeout (800ms).
    pub fn new() -> Self {
        Self {
            port: ADB_DEFAULT_PORT,
            timeout: DEFAULT_ADB_TIMEOUT,
        }
    }

    /// Set a custom target port.
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    /// Set a custom connection timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Probe a specific host IP address for ADB availability and model information.
    pub async fn probe_host(&self, ip: Ipv4Addr) -> Result<AdbDevice> {
        let addr = SocketAddr::from((ip, self.port));

        let mut stream = tokio::time::timeout(self.timeout, TcpStream::connect(addr))
            .await
            .map_err(|_| DiscoveryError::SilentProbe {
                target: format!("adb:{addr}"),
            })??;

        let request = build_adb_cnxn_packet("host::tv-rs-discovery;");
        stream.write_all(&request).await?;

        let mut buf = [0u8; 1024];
        let len = tokio::time::timeout(self.timeout, stream.read(&mut buf))
            .await
            .map_err(|_| DiscoveryError::SilentProbe {
                target: format!("adb:{addr}"),
            })??;

        if len < 24 {
            return Err(DiscoveryError::Malformed(
                "adb banner shorter than header length".to_string(),
            ));
        }

        let banner_raw = String::from_utf8_lossy(&buf[24..len]).to_string();
        let (model, manufacturer) = parse_adb_banner(&banner_raw);

        let mut fingerprint = DeviceFingerprint::new().with_open_ports([self.port]);

        if let Some(ref m) = model {
            fingerprint = fingerprint.with_adb_model(m);
        }

        Ok(AdbDevice {
            ip,
            port: self.port,
            model,
            manufacturer,
            banner: banner_raw,
            fingerprint,
        })
    }

    /// Scan a list of IP addresses for ADB devices concurrently.
    pub async fn scan_hosts(&self, ips: &[Ipv4Addr]) -> Vec<AdbDevice> {
        let mut devices = Vec::new();
        for &ip in ips {
            if let Ok(dev) = self.probe_host(ip).await {
                devices.push(dev);
            }
        }
        devices
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_adb_cnxn_packet() {
        let identity = "host::tv-rs;";
        let packet = build_adb_cnxn_packet(identity);
        assert!(packet.len() >= 24 + identity.len());

        // Command header check (first 4 bytes: "CNXN")
        assert_eq!(&packet[0..4], &ADB_CMD_CNXN.to_le_bytes());
    }

    #[test]
    fn test_parse_adb_banner() {
        let banner =
            "device::ro.product.name=aftmm;ro.product.model=AFTMM;ro.product.manufacturer=Amazon;";
        let (model, manufacturer) = parse_adb_banner(banner);

        assert_eq!(model.as_deref(), Some("AFTMM"));
        assert_eq!(manufacturer.as_deref(), Some("Amazon"));
    }

    #[tokio::test]
    async fn test_adb_mock_listener() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let port = addr.port();

        tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                let mut buf = [0u8; 256];
                let _ = socket.read(&mut buf).await;

                // Send back fake ADB CNXN header + payload
                let payload = "device::ro.product.model=SHIELD TV;ro.product.manufacturer=NVIDIA;";
                let resp = build_adb_cnxn_packet(payload);
                let _ = socket.write_all(&resp).await;
            }
        });

        let scanner = AdbScanner::new()
            .with_port(port)
            .with_timeout(Duration::from_millis(500));

        let ip = match addr.ip() {
            std::net::IpAddr::V4(ip) => ip,
            _ => panic!("Expected IPv4"),
        };

        let dev = scanner.probe_host(ip).await.expect("ADB probe failed");
        assert_eq!(dev.model.as_deref(), Some("SHIELD TV"));
        assert_eq!(dev.manufacturer.as_deref(), Some("NVIDIA"));
        assert_eq!(dev.fingerprint.adb_model.as_deref(), Some("SHIELD TV"));
    }
}
