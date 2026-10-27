//! Local HTTP display web API probe scanner.
//!
//! Scans local IP addresses across common display web management ports (80, 8060,
//! 3000, 1515, 8080, 8443), sending HTTP GET requests to detect Roku ECP, LG webOS SSAP,
//! Samsung MDC, SICP, and REST-controlled commercial signage panels.

use std::net::Ipv4Addr;
use std::time::Duration;

use reqwest::Client;

use crate::error::{DiscoveryError, Result};
use crate::fingerprint::DeviceFingerprint;

/// Commonly used HTTP display management ports.
pub const HTTP_DISPLAY_PORTS: &[u16] = &[80, 8060, 3000, 1515, 8080, 8443];

/// Default timeout for probing an HTTP display endpoint.
const DEFAULT_HTTP_TIMEOUT: Duration = Duration::from_millis(800);

/// Information gathered from probing a local HTTP display endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpDisplay {
    /// Host IPv4 address.
    pub ip: Ipv4Addr,
    /// HTTP TCP port probed.
    pub port: u16,
    /// `Server` header returned in HTTP response.
    pub server_header: Option<String>,
    /// Path probed (e.g. `"/"`, `"/query/device-info"`).
    pub endpoint_path: String,
    /// Response status code (e.g. 200).
    pub status_code: u16,
    /// Device fingerprint built from HTTP response.
    pub fingerprint: DeviceFingerprint,
}

/// Asynchronous scanner for probing local HTTP display control endpoints.
#[derive(Debug, Clone)]
pub struct LocalHttpDisplayScanner {
    timeout: Duration,
    client: Client,
}

impl Default for LocalHttpDisplayScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalHttpDisplayScanner {
    /// Create a new local HTTP display scanner.
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(DEFAULT_HTTP_TIMEOUT)
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap_or_default();

        Self {
            timeout: DEFAULT_HTTP_TIMEOUT,
            client,
        }
    }

    /// Set a custom HTTP probe timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self.client = Client::builder()
            .timeout(timeout)
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap_or_default();
        self
    }

    /// Probe a specific `IP:port` combination for HTTP display services.
    pub async fn probe_endpoint(&self, ip: Ipv4Addr, port: u16) -> Result<HttpDisplay> {
        let path = if port == 8060 {
            "/query/device-info"
        } else {
            "/"
        };

        let scheme = if port == 8443 { "https" } else { "http" };
        let url = format!("{scheme}://{ip}:{port}{path}");

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(DiscoveryError::Http)?;

        let status_code = response.status().as_u16();
        let server_header = response
            .headers()
            .get(reqwest::header::SERVER)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let mut fingerprint = DeviceFingerprint::new().with_open_ports([port]);

        if let Some(ref server) = server_header {
            fingerprint = fingerprint.with_ssdp_server(server);
        }

        Ok(HttpDisplay {
            ip,
            port,
            server_header,
            endpoint_path: path.to_string(),
            status_code,
            fingerprint,
        })
    }

    /// Scan an IP address across standard HTTP display ports.
    pub async fn scan_host(&self, ip: Ipv4Addr) -> Vec<HttpDisplay> {
        let mut found = Vec::new();
        for &port in HTTP_DISPLAY_PORTS {
            if let Ok(display) = self.probe_endpoint(ip, port).await {
                found.push(display);
            }
        }
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_probe_endpoint_mock_server() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let port = addr.port();

        tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buf = [0u8; 512];
                let _ = socket.read(&mut buf).await;
                let response = "HTTP/1.1 200 OK\r\n\
                    Server: Roku/10.0.0 UPnP/1.0\r\n\
                    Content-Length: 0\r\n\r\n";
                let _ = socket.write_all(response.as_bytes()).await;
            }
        });

        let scanner = LocalHttpDisplayScanner::new().with_timeout(Duration::from_millis(500));

        let ip = match addr.ip() {
            std::net::IpAddr::V4(ip) => ip,
            _ => panic!("Expected IPv4"),
        };

        let display = scanner
            .probe_endpoint(ip, port)
            .await
            .expect("HTTP probe failed");
        assert_eq!(display.ip, ip);
        assert_eq!(display.port, port);
        assert_eq!(
            display.server_header.as_deref(),
            Some("Roku/10.0.0 UPnP/1.0")
        );
        assert!(display.fingerprint.open_ports.contains(&port));
    }
}
