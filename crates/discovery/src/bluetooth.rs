//! Bluetooth LE advertising discovery and frame parser.
//!
//! Parses Bluetooth Low Energy (BLE) advertisement packets and Extended Inquiry
//! Response (EIR) AD structures to identify Smart TV remotes, Fire TV sticks,
//! Android TV accessories, and BLE signage beacons.

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::fingerprint::DeviceFingerprint;

/// A parsed Bluetooth Low Energy (BLE) advertisement structure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BluetoothAdvertisement {
    /// Bluetooth MAC / BD_ADDR string (e.g. `"AA:BB:CC:11:22:33"`).
    pub device_address: String,
    /// Local device name advertised over BLE (e.g. `"Fire TV Remote"`, `"Samsung Smart Control"`).
    pub name: Option<String>,
    /// Received Signal Strength Indicator in dBm (e.g. -65).
    pub rssi: i8,
    /// Manufacturer specific data payload (Company ID + vendor payload).
    pub manufacturer_data: Vec<u8>,
    /// List of advertised service UUID strings (16-bit or 128-bit).
    pub service_uuids: Vec<String>,
    /// Device fingerprint initialized with Bluetooth signatures.
    pub fingerprint: DeviceFingerprint,
}

/// Parse BLE Advertising / EIR AD structures (length-type-value fields).
pub fn parse_ble_advertisement_frame(
    device_address: &str,
    rssi: i8,
    payload: &[u8],
) -> Result<BluetoothAdvertisement> {
    let mut name = None;
    let mut manufacturer_data = Vec::new();
    let mut service_uuids = Vec::new();

    let mut offset = 0;
    while offset < payload.len() {
        let length = payload[offset] as usize;
        if length == 0 || offset + 1 + length > payload.len() {
            break;
        }

        let ad_type = payload[offset + 1];
        let data = if length > 1 {
            &payload[offset + 2..offset + 1 + length]
        } else {
            &[]
        };

        match ad_type {
            // 0x08: Shortened Local Name, 0x09: Complete Local Name
            0x08 | 0x09 => {
                if name.is_none() {
                    name = Some(String::from_utf8_lossy(data).to_string());
                }
            }
            // 0x02: Incomplete List of 16-bit Service UUIDs, 0x03: Complete List of 16-bit Service UUIDs
            0x02 | 0x03 => {
                for chunk in data.chunks(2) {
                    if chunk.len() == 2 {
                        let uuid16 = u16::from_le_bytes([chunk[0], chunk[1]]);
                        service_uuids.push(format!("0x{uuid16:04X}"));
                    }
                }
            }
            // 0x06 | 0x07: 128-bit Service Class UUIDs
            0x06 | 0x07 => {
                if data.len() == 16 {
                    let mut bytes = data.to_vec();
                    bytes.reverse(); // Convert LE to BE display string
                    let hex = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
                    service_uuids.push(hex);
                }
            }
            // 0xFF: Manufacturer Specific Data
            0xFF => {
                manufacturer_data = data.to_vec();
            }
            _ => {}
        }

        offset += length + 1;
    }

    let mut mac_prefix = device_address.to_string();
    if mac_prefix.len() >= 8 {
        mac_prefix.truncate(8);
    }

    let mut fingerprint = DeviceFingerprint::new().with_mac_pattern(mac_prefix);
    if let Some(ref n) = name {
        fingerprint = fingerprint.with_bluetooth_name(n);
    }

    Ok(BluetoothAdvertisement {
        device_address: device_address.to_string(),
        name,
        rssi,
        manufacturer_data,
        service_uuids,
        fingerprint,
    })
}

/// Bluetooth LE discovery scanner wrapper.
#[derive(Debug, Clone, Default)]
pub struct BluetoothScanner;

impl BluetoothScanner {
    /// Create a new Bluetooth scanner.
    pub fn new() -> Self {
        Self
    }

    /// Parse a raw BLE advertising frame.
    pub fn parse_frame(
        &self,
        device_address: &str,
        rssi: i8,
        payload: &[u8],
    ) -> Result<BluetoothAdvertisement> {
        parse_ble_advertisement_frame(device_address, rssi, payload)
    }

    /// Helper to construct a synthetic Bluetooth advertisement for testing or mock discovery.
    pub fn synthetic_advertisement(
        &self,
        device_address: &str,
        name: &str,
        rssi: i8,
    ) -> BluetoothAdvertisement {
        let mut mac_prefix = device_address.to_string();
        if mac_prefix.len() >= 8 {
            mac_prefix.truncate(8);
        }

        BluetoothAdvertisement {
            device_address: device_address.to_string(),
            name: Some(name.to_string()),
            rssi,
            manufacturer_data: Vec::new(),
            service_uuids: Vec::new(),
            fingerprint: DeviceFingerprint::new()
                .with_mac_pattern(mac_prefix)
                .with_bluetooth_name(name),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ble_advertisement_frame() {
        let mut frame = Vec::new();

        // AD structure 1: Complete Local Name "Fire TV Remote" (len=15, type=0x09)
        let name_bytes = b"Fire TV Remote";
        frame.push((name_bytes.len() + 1) as u8);
        frame.push(0x09);
        frame.extend_from_slice(name_bytes);

        // AD structure 2: 16-bit Service UUID 0x180F (Battery Service) (len=3, type=0x03)
        frame.push(3);
        frame.push(0x03);
        frame.extend_from_slice(&0x180Fu16.to_le_bytes());

        let adv = parse_ble_advertisement_frame("AA:BB:CC:11:22:33", -58, &frame).unwrap();

        assert_eq!(adv.device_address, "AA:BB:CC:11:22:33");
        assert_eq!(adv.name.as_deref(), Some("Fire TV Remote"));
        assert_eq!(adv.rssi, -58);
        assert!(adv.service_uuids.contains(&"0x180F".to_string()));
        assert_eq!(
            adv.fingerprint.bluetooth_name.as_deref(),
            Some("Fire TV Remote")
        );
    }

    #[test]
    fn test_parse_ble_length_one_ad_structure() {
        // Length 1 structure: length byte = 1, AD type = 0x01 (Flags), 0 data bytes
        let frame = vec![1, 0x01];
        let adv = parse_ble_advertisement_frame("11:22:33:44:55:66", -70, &frame).unwrap();
        assert_eq!(adv.device_address, "11:22:33:44:55:66");
    }

    #[test]
    fn test_synthetic_advertisement() {
        let scanner = BluetoothScanner::new();
        let adv =
            scanner.synthetic_advertisement("00:19:FB:AA:BB:CC", "Samsung Smart Control", -45);

        assert_eq!(adv.name.as_deref(), Some("Samsung Smart Control"));
        assert_eq!(adv.fingerprint.mac_pattern.as_deref(), Some("00:19:FB"));
    }
}
