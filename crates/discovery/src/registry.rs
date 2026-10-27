//! Saved device registry for managing and persisting discovered displays.
//!
//! Maintains a persistent inventory of saved devices, merging fingerprints,
//! tracking first/last seen timestamps, storing pairing tokens, and serializing
//! to JSON on disk.

use std::collections::HashMap;
use std::fs;
use std::net::Ipv4Addr;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::{DiscoveryError, Result};
use crate::fingerprint::DeviceFingerprint;
use crate::types::{DiscoveredDeviceKind, DiscoveryMethod};

fn current_unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// A saved display device entry stored in the registry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedDevice {
    /// Unique identifier for the saved device.
    pub id: String,
    /// Display friendly name (e.g. `"Living Room TV"`).
    pub friendly_name: String,
    /// Known IPv4 address.
    pub ip: Option<Ipv4Addr>,
    /// MAC address string (e.g. `"00:19:FB:12:34:56"`).
    pub mac_address: Option<String>,
    /// Device brand/category classification.
    pub kind: DiscoveredDeviceKind,
    /// Device fingerprint signatures.
    pub fingerprint: DeviceFingerprint,
    /// Methods by which this device was discovered.
    pub discovery_methods: Vec<DiscoveryMethod>,
    /// Unix timestamp when first seen.
    pub first_seen: u64,
    /// Unix timestamp when last seen/probed.
    pub last_seen: u64,
    /// Optional stored authentication or pairing token.
    pub auth_token: Option<String>,
    /// Arbitrary metadata key-value pairs.
    pub metadata: HashMap<String, String>,
}

impl SavedDevice {
    /// Create a new saved device record.
    pub fn new(
        id: impl Into<String>,
        friendly_name: impl Into<String>,
        kind: DiscoveredDeviceKind,
    ) -> Self {
        let now = current_unix_timestamp();
        Self {
            id: id.into(),
            friendly_name: friendly_name.into(),
            ip: None,
            mac_address: None,
            kind,
            fingerprint: DeviceFingerprint::new(),
            discovery_methods: Vec::new(),
            first_seen: now,
            last_seen: now,
            auth_token: None,
            metadata: HashMap::new(),
        }
    }

    /// Builder to associate an IP address.
    pub fn with_ip(mut self, ip: Ipv4Addr) -> Self {
        self.ip = Some(ip);
        self
    }

    /// Builder to associate a MAC address.
    pub fn with_mac(mut self, mac: impl Into<String>) -> Self {
        self.mac_address = Some(mac.into());
        self
    }

    /// Builder to attach a fingerprint.
    pub fn with_fingerprint(mut self, fingerprint: DeviceFingerprint) -> Self {
        self.fingerprint = fingerprint;
        self
    }

    /// Builder to add discovery method.
    pub fn with_method(mut self, method: DiscoveryMethod) -> Self {
        if !self.discovery_methods.contains(&method) {
            self.discovery_methods.push(method);
        }
        self
    }
}

/// Registry storing and managing saved display devices.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceRegistry {
    devices: HashMap<String, SavedDevice>,
}

impl DeviceRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            devices: HashMap::new(),
        }
    }

    /// Upsert a device into the registry. If a device with the same ID, MAC, or IP exists, it is merged.
    pub fn upsert(&mut self, mut device: SavedDevice) -> &SavedDevice {
        let now = current_unix_timestamp();
        device.last_seen = now;

        // Find existing key by ID, MAC, or IP
        let existing_id = self
            .devices
            .values()
            .find(|d| {
                d.id == device.id
                    || (device.mac_address.is_some() && d.mac_address == device.mac_address)
                    || (device.ip.is_some() && d.ip == device.ip)
            })
            .map(|d| d.id.clone());

        let target_id = existing_id.unwrap_or_else(|| device.id.clone());

        if let Some(existing) = self.devices.get_mut(&target_id) {
            existing.friendly_name = device.friendly_name;
            if device.ip.is_some() {
                existing.ip = device.ip;
            }
            if device.mac_address.is_some() {
                existing.mac_address = device.mac_address;
            }
            existing.kind = device.kind;
            existing.fingerprint.merge(device.fingerprint);
            for method in device.discovery_methods {
                if !existing.discovery_methods.contains(&method) {
                    existing.discovery_methods.push(method);
                }
            }
            existing.last_seen = now;
            if device.auth_token.is_some() {
                existing.auth_token = device.auth_token;
            }
            existing.metadata.extend(device.metadata);
            self.devices.get(&target_id).unwrap()
        } else {
            self.devices.insert(target_id.clone(), device);
            self.devices.get(&target_id).unwrap()
        }
    }

    /// Remove a device by ID.
    pub fn remove(&mut self, id: &str) -> Option<SavedDevice> {
        self.devices.remove(id)
    }

    /// Lookup a device by ID.
    pub fn get(&self, id: &str) -> Option<&SavedDevice> {
        self.devices.get(id)
    }

    /// List all saved devices.
    pub fn list(&self) -> Vec<&SavedDevice> {
        let mut list: Vec<&SavedDevice> = self.devices.values().collect();
        list.sort_by(|a, b| a.friendly_name.cmp(&b.friendly_name));
        list
    }

    /// Load device registry from a JSON file.
    pub fn load_from_file(path: impl AsRef<Path>) -> Result<Self> {
        let content = fs::read_to_string(path).map_err(DiscoveryError::Io)?;
        serde_json::from_str(&content).map_err(|e| DiscoveryError::Malformed(e.to_string()))
    }

    /// Save device registry to a JSON file.
    pub fn save_to_file(&self, path: impl AsRef<Path>) -> Result<()> {
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| DiscoveryError::Malformed(e.to_string()))?;
        fs::write(path, content).map_err(DiscoveryError::Io)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_registry_upsert_and_merge() {
        let mut registry = DeviceRegistry::new();

        let dev1 = SavedDevice::new(
            "tv-1",
            "Living Room TV",
            DiscoveredDeviceKind::TvBrand("Samsung Tizen TV".to_string()),
        )
        .with_ip(Ipv4Addr::new(192, 168, 1, 10))
        .with_method(DiscoveryMethod::Ssdp);

        registry.upsert(dev1);
        assert_eq!(registry.list().len(), 1);

        // Update dev1 with MAC address and additional method
        let dev2 = SavedDevice::new(
            "tv-1",
            "Living Room TV Updated",
            DiscoveredDeviceKind::TvBrand("Samsung Tizen TV".to_string()),
        )
        .with_ip(Ipv4Addr::new(192, 168, 1, 10))
        .with_mac("00:19:FB:00:11:22")
        .with_method(DiscoveryMethod::Mdns);

        registry.upsert(dev2);
        assert_eq!(registry.list().len(), 1);

        let saved = registry.get("tv-1").unwrap();
        assert_eq!(saved.friendly_name, "Living Room TV Updated");
        assert_eq!(saved.mac_address.as_deref(), Some("00:19:FB:00:11:22"));
        assert!(saved.discovery_methods.contains(&DiscoveryMethod::Ssdp));
        assert!(saved.discovery_methods.contains(&DiscoveryMethod::Mdns));
    }

    #[test]
    fn test_device_registry_file_roundtrip() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("tv_rs_device_registry_test.json");

        let mut registry = DeviceRegistry::new();
        let dev = SavedDevice::new(
            "signage-100",
            "Lobby Signage",
            DiscoveredDeviceKind::CommercialPanel("SICP Panel".to_string()),
        )
        .with_ip(Ipv4Addr::new(192, 168, 1, 50))
        .with_method(DiscoveryMethod::LocalHttp);

        registry.upsert(dev);
        registry.save_to_file(&file_path).unwrap();

        let loaded = DeviceRegistry::load_from_file(&file_path).unwrap();
        assert_eq!(loaded.list().len(), 1);
        assert_eq!(
            loaded.get("signage-100").unwrap().friendly_name,
            "Lobby Signage"
        );

        let _ = fs::remove_file(file_path);
    }
}
