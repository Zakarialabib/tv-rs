//! Google Cast / Chromecast service discovery module.
//!
//! Scans for Google Cast devices and Chromecast receivers over mDNS service
//! `_googlecast._tcp.local` and DIAL / Cast HTTP endpoints (ports 8008 / 8009),
//! extracting friendly device names, models, and unique device IDs.

use std::net::Ipv4Addr;
use std::time::Duration;

use crate::error::Result;
use crate::fingerprint::DeviceFingerprint;
use crate::mdns::{MdnsScanner, MdnsServiceRecord};

/// Default mDNS service type for Google Cast receivers.
pub const CAST_MDNS_SERVICE_TYPE: &str = "_googlecast._tcp.local";

/// Default HTTP port used by Google Cast devices.
pub const CAST_HTTP_PORT: u16 = 8008;
/// Default TLS port used by Google Cast devices.
pub const CAST_TLS_PORT: u16 = 8009;

/// Information extracted from a discovered Google Cast device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CastDevice {
    /// Friendly device name (e.g. `"Living Room TV"`).
    pub friendly_name: String,
    /// Product model name (e.g. `"Chromecast"`, `"Chromecast with Google TV"`).
    pub model_name: String,
    /// Unique device ID or UUID.
    pub id: String,
    /// Host IPv4 address.
    pub ip: Ipv4Addr,
    /// Cast control service port (8009 for TLS/CastV2, 8008 for HTTP).
    pub port: u16,
    /// Populated device fingerprint.
    pub fingerprint: DeviceFingerprint,
}

/// Parse mDNS TXT records into Cast device details (`fn` = friendly name, `md` = model, `id` = device ID).
pub fn parse_cast_record(record: &MdnsServiceRecord) -> CastDevice {
    let friendly_name = record
        .txt_records
        .get("fn")
        .cloned()
        .unwrap_or_else(|| record.service_name.clone());

    let model_name = record
        .txt_records
        .get("md")
        .cloned()
        .unwrap_or_else(|| "Chromecast".to_string());

    let id = record
        .txt_records
        .get("id")
        .cloned()
        .unwrap_or_else(|| record.host_name.clone());

    let fingerprint = DeviceFingerprint::new()
        .with_mdns_name(&record.service_name)
        .with_open_ports([CAST_HTTP_PORT, CAST_TLS_PORT]);

    CastDevice {
        friendly_name,
        model_name,
        id,
        ip: record.ip,
        port: record.port,
        fingerprint,
    }
}

/// Asynchronous Google Cast device scanner.
#[derive(Debug, Clone, Default)]
pub struct CastScanner {
    mdns_scanner: MdnsScanner,
}

impl CastScanner {
    /// Create a new Cast scanner.
    pub fn new() -> Self {
        Self {
            mdns_scanner: MdnsScanner::new(),
        }
    }

    /// Set a custom discovery timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.mdns_scanner = self.mdns_scanner.with_timeout(timeout);
        self
    }

    /// Discover Google Cast devices on the local network via mDNS.
    pub async fn discover_cast_devices(&self) -> Result<Vec<CastDevice>> {
        let records = self
            .mdns_scanner
            .discover_services(CAST_MDNS_SERVICE_TYPE)
            .await?;

        Ok(records.iter().map(parse_cast_record).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cast_record() {
        let mut record = MdnsServiceRecord::new(
            "Chromecast-1234._googlecast._tcp.local",
            CAST_MDNS_SERVICE_TYPE,
            "chromecast.local",
            8009,
            Ipv4Addr::new(192, 168, 1, 80),
        );
        record
            .txt_records
            .insert("fn".to_string(), "Office Screen".to_string());
        record
            .txt_records
            .insert("md".to_string(), "Chromecast with Google TV".to_string());
        record
            .txt_records
            .insert("id".to_string(), "uuid-1234-5678".to_string());

        let cast_dev = parse_cast_record(&record);

        assert_eq!(cast_dev.friendly_name, "Office Screen");
        assert_eq!(cast_dev.model_name, "Chromecast with Google TV");
        assert_eq!(cast_dev.id, "uuid-1234-5678");
        assert_eq!(cast_dev.ip, Ipv4Addr::new(192, 168, 1, 80));
        assert_eq!(cast_dev.port, 8009);
        assert!(cast_dev.fingerprint.open_ports.contains(&8008));
        assert!(cast_dev.fingerprint.open_ports.contains(&8009));
    }
}
