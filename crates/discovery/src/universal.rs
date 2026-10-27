//! Universal display discovery scanner and aggregator.
//!
//! Coordinates SSDP, mDNS, ADB, Bluetooth LE advertising, Google Cast, local HTTP probes,
//! and HDMI-CEC topology mapping into a unified discovery workflow that generates device
//! fingerprints and classifies display brands and platforms.

use std::net::Ipv4Addr;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::adb::AdbScanner;
use crate::bluetooth::{BluetoothAdvertisement, BluetoothScanner};
use crate::cast::CastScanner;
use crate::cec_topology::CecTopologyScanner;
use crate::client::SsdpScanner;
use crate::fingerprint::DeviceFingerprint;
use crate::http_display::LocalHttpDisplayScanner;
use crate::mdns::{MdnsScanner, MDNS_SERVICE_TYPE_SIGNAGE, MDNS_SERVICE_TYPE_TV};
use crate::registry::{DeviceRegistry, SavedDevice};
use crate::types::{DiscoveredDeviceKind, DiscoveryMethod};

/// A unified display device discovered by the [`UniversalScanner`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveredDevice {
    /// Unique identifier for the device (e.g., USN, UUID, IP, or Bluetooth MAC).
    pub id: String,
    /// Friendly device name (e.g. `"Living Room Smart TV"`).
    pub friendly_name: String,
    /// Primary IPv4 address if network-attached.
    pub ip: Option<Ipv4Addr>,
    /// Classified device brand or category (Samsung Tizen, LG webOS, Fire TV, etc.).
    pub kind: DiscoveredDeviceKind,
    /// Unified device fingerprint aggregated across protocols.
    pub fingerprint: DeviceFingerprint,
    /// List of discovery mechanisms that detected this device.
    pub discovery_methods: Vec<DiscoveryMethod>,
}

impl DiscoveredDevice {
    /// Convert this discovered device into a [`SavedDevice`] record for persistence.
    pub fn into_saved_device(self) -> SavedDevice {
        SavedDevice::new(self.id, self.friendly_name, self.kind)
            .with_fingerprint(self.fingerprint)
            .with_ip(self.ip.unwrap_or(Ipv4Addr::UNSPECIFIED))
    }
}

/// Master scanner coordinating discovery across all supported mechanisms.
#[derive(Debug, Clone, Default)]
pub struct UniversalScanner {
    ssdp: SsdpScanner,
    mdns: MdnsScanner,
    adb: AdbScanner,
    bluetooth: BluetoothScanner,
    cast: CastScanner,
    http_display: LocalHttpDisplayScanner,
    cec: CecTopologyScanner,
    registry: DeviceRegistry,
}

impl UniversalScanner {
    /// Create a new universal scanner instance.
    pub fn new() -> Self {
        Self {
            ssdp: SsdpScanner::new(),
            mdns: MdnsScanner::new(),
            adb: AdbScanner::new(),
            bluetooth: BluetoothScanner::new(),
            cast: CastScanner::new(),
            http_display: LocalHttpDisplayScanner::new(),
            cec: CecTopologyScanner::new(),
            registry: DeviceRegistry::new(),
        }
    }

    /// Access the SSDP scanner.
    pub fn ssdp(&self) -> &SsdpScanner {
        &self.ssdp
    }

    /// Access the mDNS scanner.
    pub fn mdns(&self) -> &MdnsScanner {
        &self.mdns
    }

    /// Access the ADB scanner.
    pub fn adb(&self) -> &AdbScanner {
        &self.adb
    }

    /// Access the Bluetooth scanner.
    pub fn bluetooth(&self) -> &BluetoothScanner {
        &self.bluetooth
    }

    /// Access the Cast scanner.
    pub fn cast(&self) -> &CastScanner {
        &self.cast
    }

    /// Access the Local HTTP display scanner.
    pub fn http_display(&self) -> &LocalHttpDisplayScanner {
        &self.http_display
    }

    /// Access the HDMI-CEC topology scanner.
    pub fn cec(&self) -> &CecTopologyScanner {
        &self.cec
    }

    /// Access the internal saved device registry.
    pub fn registry(&self) -> &DeviceRegistry {
        &self.registry
    }

    /// Access the mutable saved device registry.
    pub fn registry_mut(&mut self) -> &mut DeviceRegistry {
        &mut self.registry
    }

    /// Set discovery timeout across sub-scanners.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.ssdp = self.ssdp.with_timeout(timeout);
        self.mdns = self.mdns.with_timeout(timeout);
        self.adb = self.adb.with_timeout(timeout);
        self.cast = self.cast.with_timeout(timeout);
        self.http_display = self.http_display.with_timeout(timeout);
        self
    }

    /// Process raw SSDP and mDNS results into [`DiscoveredDevice`] records.
    pub fn aggregate_network_results(
        &self,
        ssdp_devices: &[(
            crate::payload::SsdpMsearchResponse,
            crate::payload::SsdpDeviceDescriptor,
        )],
        mdns_records: &[crate::mdns::MdnsServiceRecord],
    ) -> Vec<DiscoveredDevice> {
        let mut results: Vec<DiscoveredDevice> = Vec::new();

        // 1. Process SSDP results
        for (resp, desc) in ssdp_devices {
            let id = resp.usn.clone().unwrap_or_else(|| resp.source.clone());
            let friendly_name = desc
                .friendly_name
                .clone()
                .unwrap_or_else(|| "Discovered TV".to_string());

            let ip: Option<Ipv4Addr> = resp.source.parse().ok();

            let fingerprint = DeviceFingerprint::new()
                .with_ssdp_server(resp.server.clone().unwrap_or_default())
                .with_mdns_name(friendly_name.clone());

            let kind = fingerprint.identify();

            results.push(DiscoveredDevice {
                id,
                friendly_name,
                ip,
                kind,
                fingerprint,
                discovery_methods: vec![DiscoveryMethod::Ssdp],
            });
        }

        // 2. Process mDNS results & merge by IP
        for record in mdns_records {
            let friendly_name = record.service_name.clone();
            let ip = Some(record.ip);

            if let Some(existing) = results.iter_mut().find(|d| d.ip == ip) {
                existing.fingerprint.mdns_name = Some(record.service_name.clone());
                if !existing.discovery_methods.contains(&DiscoveryMethod::Mdns) {
                    existing.discovery_methods.push(DiscoveryMethod::Mdns);
                }
                existing.kind = existing.fingerprint.identify();
            } else {
                let fingerprint = DeviceFingerprint::new().with_mdns_name(&record.service_name);
                let kind = fingerprint.identify();

                results.push(DiscoveredDevice {
                    id: record.service_name.clone(),
                    friendly_name,
                    ip,
                    kind,
                    fingerprint,
                    discovery_methods: vec![DiscoveryMethod::Mdns],
                });
            }
        }

        results
    }

    /// Process a Bluetooth advertisement and add/merge into discovered devices.
    pub fn process_bluetooth_advertisement(
        &self,
        devices: &mut Vec<DiscoveredDevice>,
        adv: &BluetoothAdvertisement,
    ) {
        if let Some(existing) = devices.iter_mut().find(|d| d.id == adv.device_address) {
            existing.fingerprint.merge(adv.fingerprint.clone());
            if !existing
                .discovery_methods
                .contains(&DiscoveryMethod::Bluetooth)
            {
                existing.discovery_methods.push(DiscoveryMethod::Bluetooth);
            }
            existing.kind = existing.fingerprint.identify();
        } else {
            let friendly_name = adv
                .name
                .clone()
                .unwrap_or_else(|| format!("BLE Device {}", adv.device_address));

            let kind = adv.fingerprint.identify();

            devices.push(DiscoveredDevice {
                id: adv.device_address.clone(),
                friendly_name,
                ip: None,
                kind,
                fingerprint: adv.fingerprint.clone(),
                discovery_methods: vec![DiscoveryMethod::Bluetooth],
            });
        }
    }

    /// Process HDMI-CEC topology scan output and add/merge into discovered devices.
    pub fn process_cec_topology(&self, devices: &mut Vec<DiscoveredDevice>, output: &str) {
        let map = self.cec.parse_topology(output);
        for node in map.nodes {
            let id = format!("cec-node-{}", node.logical_address);
            let name = node
                .vendor_name
                .clone()
                .unwrap_or_else(|| format!("CEC Display {}", node.logical_address));

            let kind = node.fingerprint.identify();

            devices.push(DiscoveredDevice {
                id,
                friendly_name: name,
                ip: None,
                kind,
                fingerprint: node.fingerprint,
                discovery_methods: vec![DiscoveryMethod::CecTopology],
            });
        }
    }

    /// Execute SSDP and mDNS discovery and return aggregated discovered devices.
    pub async fn discover_network_devices(&self) -> Vec<DiscoveredDevice> {
        let ssdp_results = self.ssdp.discover_tvs().await.unwrap_or_default();

        let mdns_tv = self
            .mdns
            .discover_services(MDNS_SERVICE_TYPE_TV)
            .await
            .unwrap_or_default();

        let mdns_signage = self
            .mdns
            .discover_services(MDNS_SERVICE_TYPE_SIGNAGE)
            .await
            .unwrap_or_default();

        let mut all_mdns = mdns_tv;
        all_mdns.extend(mdns_signage);

        self.aggregate_network_results(&ssdp_results, &all_mdns)
    }

    /// Execute universal discovery across network protocols and update the saved device registry.
    pub async fn scan_and_save(&mut self) -> Vec<SavedDevice> {
        let discovered = self.discover_network_devices().await;
        let mut saved = Vec::new();

        for dev in discovered {
            let saved_dev = dev.into_saved_device();
            let entry = self.registry.upsert(saved_dev).clone();
            saved.push(entry);
        }

        saved
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn test_universal_scanner_bluetooth_and_cec_processing() {
        let scanner = UniversalScanner::new();
        let mut devices = Vec::new();

        // 1. Process Bluetooth advertisement
        let adv =
            scanner
                .bluetooth
                .synthetic_advertisement("AA:BB:CC:DD:EE:FF", "Fire TV Remote", -50);
        scanner.process_bluetooth_advertisement(&mut devices, &adv);

        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].friendly_name, "Fire TV Remote");
        assert!(matches!(devices[0].kind, DiscoveredDeviceKind::FireTv(_)));

        // 2. Process CEC topology
        let cec_output = r#"
device #0
  logical address: 0
  physical address: 0.0.0.0
  vendor id: 0x0000F0
"#;
        scanner.process_cec_topology(&mut devices, cec_output);

        assert_eq!(devices.len(), 2);
        assert_eq!(devices[1].id, "cec-node-0");
        assert_eq!(devices[1].friendly_name, "Samsung (Anynet+)");
        assert!(matches!(devices[1].kind, DiscoveredDeviceKind::TvBrand(_)));
    }

    #[test]
    fn test_discovered_device_into_saved_device() {
        let dev = DiscoveredDevice {
            id: "usn-123".to_string(),
            friendly_name: "Boardroom Screen".to_string(),
            ip: Some(Ipv4Addr::new(192, 168, 1, 100)),
            kind: DiscoveredDeviceKind::CommercialPanel("Samsung MDC".to_string()),
            fingerprint: DeviceFingerprint::new().with_open_ports([1515]),
            discovery_methods: vec![DiscoveryMethod::Ssdp],
        };

        let saved = dev.into_saved_device();
        assert_eq!(saved.id, "usn-123");
        assert_eq!(saved.friendly_name, "Boardroom Screen");
        assert_eq!(saved.ip, Some(Ipv4Addr::new(192, 168, 1, 100)));
        assert_eq!(saved.fingerprint.open_ports, vec![1515]);
    }
}
