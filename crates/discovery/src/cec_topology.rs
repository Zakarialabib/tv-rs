//! HDMI-CEC topology discovery and vendor ID mapper.
//!
//! Maps HDMI-CEC logical and physical address hierarchies (e.g. TV at 0, source
//! at 1.0.0.0), extracting CEC Vendor IDs (Anynet+, SimpLink, Bravia Sync) and
//! constructing device fingerprints.

use serde::{Deserialize, Serialize};

use crate::fingerprint::DeviceFingerprint;

/// Translate 24-bit IEEE OUI CEC Vendor ID to known vendor/brand string.
pub fn vendor_id_to_name(vendor_id: u32) -> Option<&'static str> {
    match vendor_id {
        0x0000F0 | 0x000030 => Some("Samsung (Anynet+)"),
        0x00E091 => Some("LG (SimpLink)"),
        0x0008A1 => Some("Sony (Bravia Sync)"),
        0x0000C0 => Some("Roku"),
        0x001099 => Some("Panasonic (VIERA Link)"),
        0x000039 => Some("Toshiba (Regza Link)"),
        0x00E036 => Some("Pioneer (Kuro Link)"),
        0x000B57 => Some("Philips (EasyLink)"),
        _ => None,
    }
}

/// A node in the HDMI-CEC topology map representing a connected display or source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CecDeviceNode {
    /// Logical address on the CEC bus (0..=15).
    pub logical_address: u8,
    /// Physical address encoded as u16 (e.g. 0x1000 for "1.0.0.0").
    pub physical_address: u16,
    /// 24-bit IEEE OUI CEC vendor ID.
    pub vendor_id: Option<u32>,
    /// Vendor or brand string associated with the CEC vendor ID.
    pub vendor_name: Option<String>,
    /// Device fingerprint populated with CEC signatures.
    pub fingerprint: DeviceFingerprint,
}

impl CecDeviceNode {
    /// Create a new CEC topology node.
    pub fn new(logical_address: u8, physical_address: u16) -> Self {
        Self {
            logical_address,
            physical_address,
            vendor_id: None,
            vendor_name: None,
            fingerprint: DeviceFingerprint::new(),
        }
    }

    /// Set the CEC Vendor ID and populate fingerprint.
    pub fn with_vendor_id(mut self, vendor_id: u32) -> Self {
        self.vendor_id = Some(vendor_id);
        if let Some(name) = vendor_id_to_name(vendor_id) {
            self.vendor_name = Some(name.to_string());
        }
        self.fingerprint = self.fingerprint.with_cec_vendor_id(vendor_id);
        self
    }

    /// Format physical address as dotted string (e.g. `"1.0.0.0"`).
    pub fn physical_address_string(&self) -> String {
        let a = (self.physical_address >> 12) & 0xF;
        let b = (self.physical_address >> 8) & 0xF;
        let c = (self.physical_address >> 4) & 0xF;
        let d = self.physical_address & 0xF;
        format!("{a}.{b}.{c}.{d}")
    }
}

/// HDMI-CEC bus topology map.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CecTopologyMap {
    /// Discovered devices on the CEC bus indexed by logical address.
    pub nodes: Vec<CecDeviceNode>,
}

impl CecTopologyMap {
    /// Create an empty topology map.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a node to the topology map.
    pub fn add_node(&mut self, node: CecDeviceNode) {
        if !self
            .nodes
            .iter()
            .any(|n| n.logical_address == node.logical_address)
        {
            self.nodes.push(node);
        }
    }

    /// Parse CEC topology output text (such as output from `cec-client` or CEC adapter dump).
    pub fn parse_cec_output(output: &str) -> Self {
        let mut map = Self::new();
        let mut current_addr = None;
        let mut current_phys = None;
        let mut current_vendor = None;

        for line in output.lines() {
            let line = line.trim();
            if line.starts_with("device #") || line.starts_with("CEC bus information") {
                if let (Some(la), Some(pa)) = (current_addr, current_phys) {
                    let mut node = CecDeviceNode::new(la, pa);
                    if let Some(vid) = current_vendor {
                        node = node.with_vendor_id(vid);
                    }
                    map.add_node(node);
                }
                current_addr = None;
                current_phys = None;
                current_vendor = None;
            }

            if line.contains("address:") || line.contains("logical address:") {
                if let Some((_, val)) = line.rsplit_once(':') {
                    if let Ok(addr) = val.trim().parse::<u8>() {
                        current_addr = Some(addr);
                    }
                }
            }

            if line.contains("physical address:") {
                if let Some((_, val)) = line.rsplit_once(':') {
                    let clean = val.replace(['.', ':', ' '], "");
                    if let Ok(pa) = u16::from_str_radix(&clean, 16) {
                        current_phys = Some(pa);
                    }
                }
            }

            if line.contains("vendor id:") || line.contains("vendor:") {
                if let Some((_, val)) = line.rsplit_once(':') {
                    let clean = val.trim().strip_prefix("0x").unwrap_or(val.trim());
                    if let Ok(vid) = u32::from_str_radix(clean, 16) {
                        current_vendor = Some(vid);
                    }
                }
            }
        }

        if let (Some(la), Some(pa)) = (current_addr, current_phys) {
            let mut node = CecDeviceNode::new(la, pa);
            if let Some(vid) = current_vendor {
                node = node.with_vendor_id(vid);
            }
            map.add_node(node);
        }

        map
    }
}

/// HDMI-CEC topology scanner.
#[derive(Debug, Clone, Default)]
pub struct CecTopologyScanner;

impl CecTopologyScanner {
    /// Create a new CEC topology scanner.
    pub fn new() -> Self {
        Self
    }

    /// Parse CEC topology output into a [`CecTopologyMap`].
    pub fn parse_topology(&self, output: &str) -> CecTopologyMap {
        CecTopologyMap::parse_cec_output(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cec_device_node_formatting() {
        let node = CecDeviceNode::new(0, 0x1000).with_vendor_id(0x0000F0);

        assert_eq!(node.logical_address, 0);
        assert_eq!(node.physical_address_string(), "1.0.0.0");
        assert_eq!(node.vendor_id, Some(0x0000F0));
        assert_eq!(node.vendor_name.as_deref(), Some("Samsung (Anynet+)"));
        assert_eq!(node.fingerprint.cec_vendor_id, Some(0x0000F0));
    }

    #[test]
    fn test_parse_cec_topology_output() {
        let output = r#"
device #0
  logical address: 0
  physical address: 0.0.0.0
  vendor id: 0x0000F0
device #1
  logical address: 4
  physical address: 1.0.0.0
  vendor id: 0x00E091
"#;

        let map = CecTopologyMap::parse_cec_output(output);
        assert_eq!(map.nodes.len(), 2);

        assert_eq!(map.nodes[0].logical_address, 0);
        assert_eq!(
            map.nodes[0].vendor_name.as_deref(),
            Some("Samsung (Anynet+)")
        );

        assert_eq!(map.nodes[1].logical_address, 4);
        assert_eq!(map.nodes[1].physical_address_string(), "1.0.0.0");
        assert_eq!(map.nodes[1].vendor_name.as_deref(), Some("LG (SimpLink)"));
    }
}
