//! Device fingerprinting and brand/category classification engine.
//!
//! Collects signatures across MAC address, mDNS hostname, SSDP server string,
//! open TCP ports, HDMI-CEC vendor IDs, Bluetooth names, and ADB model strings,
//! then classifies the display into brand or platform categories.

use serde::{Deserialize, Serialize};

use crate::types::DiscoveredDeviceKind;

/// A unified device fingerprint collected across multiple discovery protocols.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceFingerprint {
    /// MAC address pattern or OUI prefix (e.g., "00:19:FB", "AA:BB:CC").
    pub mac_pattern: Option<String>,
    /// mDNS host name or service instance name (e.g., "Living-Room-TV.local", "_googlecast._tcp.local").
    pub mdns_name: Option<String>,
    /// SSDP `SERVER` header string or UPnP server description (e.g., "WebOS/1.0 UPnP/1.0 LG Electronics").
    pub ssdp_server: Option<String>,
    /// List of open TCP/UDP ports detected on the device (e.g., [5555, 8001, 8060, 1515]).
    pub open_ports: Vec<u16>,
    /// HDMI-CEC Vendor ID (24-bit IEEE OUI, e.g., 0x0000F0 for Samsung, 0x00E091 for LG).
    pub cec_vendor_id: Option<u32>,
    /// Bluetooth LE advertising device name or remote control string (e.g., "Fire TV Remote", "Samsung Smart Control").
    pub bluetooth_name: Option<String>,
    /// ADB device model string from `ro.product.model` or `getprop` (e.g., "AFTMM", "SHIELD Android TV", "Amlogic S905X").
    pub adb_model: Option<String>,
}

impl DeviceFingerprint {
    /// Create a new empty device fingerprint.
    pub fn new() -> Self {
        Self::default()
    }

    /// Builder helper to set the MAC address pattern.
    pub fn with_mac_pattern(mut self, mac: impl Into<String>) -> Self {
        self.mac_pattern = Some(mac.into());
        self
    }

    /// Builder helper to set the mDNS name.
    pub fn with_mdns_name(mut self, mdns: impl Into<String>) -> Self {
        self.mdns_name = Some(mdns.into());
        self
    }

    /// Builder helper to set the SSDP server string.
    pub fn with_ssdp_server(mut self, ssdp: impl Into<String>) -> Self {
        self.ssdp_server = Some(ssdp.into());
        self
    }

    /// Builder helper to add open ports.
    pub fn with_open_ports(mut self, ports: impl IntoIterator<Item = u16>) -> Self {
        self.open_ports.extend(ports);
        self.open_ports.sort_unstable();
        self.open_ports.dedup();
        self
    }

    /// Builder helper to set the HDMI-CEC Vendor ID.
    pub fn with_cec_vendor_id(mut self, vendor_id: u32) -> Self {
        self.cec_vendor_id = Some(vendor_id);
        self
    }

    /// Builder helper to set the Bluetooth advertising name.
    pub fn with_bluetooth_name(mut self, bt_name: impl Into<String>) -> Self {
        self.bluetooth_name = Some(bt_name.into());
        self
    }

    /// Builder helper to set the ADB model string.
    pub fn with_adb_model(mut self, adb: impl Into<String>) -> Self {
        self.adb_model = Some(adb.into());
        self
    }

    /// Merge another fingerprint into this one, overwriting `None` values and combining open ports.
    pub fn merge(&mut self, other: DeviceFingerprint) {
        if self.mac_pattern.is_none() {
            self.mac_pattern = other.mac_pattern;
        }
        if self.mdns_name.is_none() {
            self.mdns_name = other.mdns_name;
        }
        if self.ssdp_server.is_none() {
            self.ssdp_server = other.ssdp_server;
        }
        if self.cec_vendor_id.is_none() {
            self.cec_vendor_id = other.cec_vendor_id;
        }
        if self.bluetooth_name.is_none() {
            self.bluetooth_name = other.bluetooth_name;
        }
        if self.adb_model.is_none() {
            self.adb_model = other.adb_model;
        }
        self.open_ports.extend(other.open_ports);
        self.open_ports.sort_unstable();
        self.open_ports.dedup();
    }

    /// Classify this fingerprint into a specific device category or brand.
    pub fn identify(&self) -> DiscoveredDeviceKind {
        let adb = self.adb_model.as_deref().unwrap_or("").to_ascii_lowercase();
        let bt = self
            .bluetooth_name
            .as_deref()
            .unwrap_or("")
            .to_ascii_lowercase();
        let mdns = self.mdns_name.as_deref().unwrap_or("").to_ascii_lowercase();
        let ssdp = self
            .ssdp_server
            .as_deref()
            .unwrap_or("")
            .to_ascii_lowercase();
        let mac = self
            .mac_pattern
            .as_deref()
            .unwrap_or("")
            .to_ascii_uppercase();

        // 1. Amazon Fire TV check
        if adb.contains("aft")
            || adb.contains("fire")
            || bt.contains("fire tv")
            || bt.contains("amazon")
            || mdns.contains("firetv")
            || mdns.contains("amazon")
            || ssdp.contains("amazon")
        {
            let model = if !adb.is_empty() {
                self.adb_model.clone().unwrap()
            } else {
                "Amazon Fire TV".to_string()
            };
            return DiscoveredDeviceKind::FireTv(model);
        }

        // 2. Commercial / Industrial Display Panel check
        if self.open_ports.contains(&1515) // Samsung MDC TCP port
            || ssdp.contains("mdc")
            || ssdp.contains("signage")
            || ssdp.contains("sicp")
            || mdns.contains("signage")
            || mdns.contains("sicp")
            || mdns.contains("commercial")
            || bt.contains("commercial panel")
            || mac.starts_with("00:19:FB")
        // Samsung Commercial OUI
        {
            let name = if ssdp.contains("sicp") || mdns.contains("sicp") {
                "SICP Commercial Panel".to_string()
            } else if self.open_ports.contains(&1515) || ssdp.contains("mdc") {
                "Samsung MDC Signage Panel".to_string()
            } else if ssdp.contains("lg") || mdns.contains("lg") {
                "LG WebOS Commercial Signage".to_string()
            } else {
                "Commercial Display Panel".to_string()
            };
            return DiscoveredDeviceKind::CommercialPanel(name);
        }

        // 3. Amlogic Platform Device check
        if adb.contains("amlogic")
            || adb.contains("s905")
            || adb.contains("s912")
            || adb.contains("s922")
            || adb.contains("p212")
            || adb.contains("droidlogic")
            || mdns.contains("amlogic")
            || ssdp.contains("amlogic")
        {
            let model = if !adb.is_empty() {
                self.adb_model.clone().unwrap()
            } else {
                "Amlogic Reference Device".to_string()
            };
            return DiscoveredDeviceKind::AmlogicDevice(model);
        }

        // 4. Chromecast-like / Google Cast Device check
        if self.open_ports.contains(&8008)
            || self.open_ports.contains(&8009)
            || mdns.contains("chromecast")
            || mdns.contains("googlecast")
            || mdns.contains("eureka")
            || ssdp.contains("chromecast")
            || ssdp.contains("google inc")
        {
            let model = if mdns.contains("chromecast") || ssdp.contains("chromecast") {
                "Google Chromecast".to_string()
            } else {
                "Google Cast Display".to_string()
            };
            return DiscoveredDeviceKind::ChromecastLike(model);
        }

        // 5. Android TV Box check
        if adb.contains("shield")
            || adb.contains("nvidia")
            || adb.contains("mi box")
            || adb.contains("mibox")
            || adb.contains("android tv")
            || adb.contains("nexus player")
            || bt.contains("shield remote")
            || bt.contains("android tv")
            || (self.open_ports.contains(&5555) && !adb.is_empty())
        {
            let model = if !adb.is_empty() {
                self.adb_model.clone().unwrap()
            } else if bt.contains("shield") {
                "NVIDIA SHIELD TV".to_string()
            } else {
                "Android TV Box".to_string()
            };
            return DiscoveredDeviceKind::AndroidBox(model);
        }

        // 6. Consumer Smart TV Brands check
        // Samsung Tizen
        if ssdp.contains("samsung")
            || self.open_ports.contains(&8001)
            || self.open_ports.contains(&8002)
            || self.cec_vendor_id == Some(0x0000F0)
            || self.cec_vendor_id == Some(0x000030)
            || bt.contains("samsung")
        {
            return DiscoveredDeviceKind::TvBrand("Samsung Tizen TV".to_string());
        }

        // LG webOS
        if ssdp.contains("webos")
            || ssdp.contains("lg electronics")
            || self.open_ports.contains(&3000)
            || self.open_ports.contains(&3001)
            || self.cec_vendor_id == Some(0x00E091)
            || bt.contains("lg tv")
        {
            return DiscoveredDeviceKind::TvBrand("LG webOS TV".to_string());
        }

        // Roku TV
        if self.open_ports.contains(&8060)
            || ssdp.contains("roku")
            || mdns.contains("roku")
            || self.cec_vendor_id == Some(0x0000C0)
        {
            return DiscoveredDeviceKind::TvBrand("Roku TV".to_string());
        }

        // Sony Bravia
        if ssdp.contains("sony") || mdns.contains("bravia") || self.cec_vendor_id == Some(0x0008A1)
        {
            return DiscoveredDeviceKind::TvBrand("Sony Bravia TV".to_string());
        }

        // Fallback for generic or unidentified displays
        DiscoveredDeviceKind::Unknown("Generic Display".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identify_fire_tv() {
        let fp = DeviceFingerprint::new()
            .with_adb_model("AFTMM")
            .with_bluetooth_name("Fire TV Remote");
        assert_eq!(
            fp.identify(),
            DiscoveredDeviceKind::FireTv("AFTMM".to_string())
        );
    }

    #[test]
    fn test_identify_commercial_panel() {
        let fp_mdc = DeviceFingerprint::new()
            .with_open_ports([1515])
            .with_ssdp_server("Samsung MDC Server v1.0");
        assert_eq!(
            fp_mdc.identify(),
            DiscoveredDeviceKind::CommercialPanel("Samsung MDC Signage Panel".to_string())
        );

        let fp_sicp = DeviceFingerprint::new().with_mdns_name("sicp-display.local");
        assert_eq!(
            fp_sicp.identify(),
            DiscoveredDeviceKind::CommercialPanel("SICP Commercial Panel".to_string())
        );
    }

    #[test]
    fn test_identify_amlogic_device() {
        let fp = DeviceFingerprint::new()
            .with_adb_model("Amlogic S905X4 Box")
            .with_open_ports([5555]);
        assert_eq!(
            fp.identify(),
            DiscoveredDeviceKind::AmlogicDevice("Amlogic S905X4 Box".to_string())
        );
    }

    #[test]
    fn test_identify_chromecast() {
        let fp = DeviceFingerprint::new()
            .with_open_ports([8008, 8009])
            .with_mdns_name("_googlecast._tcp.local");
        assert_eq!(
            fp.identify(),
            DiscoveredDeviceKind::ChromecastLike("Google Cast Display".to_string())
        );
    }

    #[test]
    fn test_identify_android_box() {
        let fp = DeviceFingerprint::new()
            .with_adb_model("NVIDIA SHIELD Android TV")
            .with_open_ports([5555]);
        assert_eq!(
            fp.identify(),
            DiscoveredDeviceKind::AndroidBox("NVIDIA SHIELD Android TV".to_string())
        );
    }

    #[test]
    fn test_identify_tv_brands() {
        // Samsung
        let fp_samsung = DeviceFingerprint::new()
            .with_ssdp_server("Samsung UPnP SDK")
            .with_open_ports([8001]);
        assert_eq!(
            fp_samsung.identify(),
            DiscoveredDeviceKind::TvBrand("Samsung Tizen TV".to_string())
        );

        // LG
        let fp_lg = DeviceFingerprint::new()
            .with_ssdp_server("WebOS/1.0 UPnP/1.0 LG Electronics")
            .with_open_ports([3000]);
        assert_eq!(
            fp_lg.identify(),
            DiscoveredDeviceKind::TvBrand("LG webOS TV".to_string())
        );

        // Roku
        let fp_roku = DeviceFingerprint::new().with_open_ports([8060]);
        assert_eq!(
            fp_roku.identify(),
            DiscoveredDeviceKind::TvBrand("Roku TV".to_string())
        );
    }

    #[test]
    fn test_fingerprint_merge() {
        let mut fp1 = DeviceFingerprint::new().with_open_ports([8001]);
        let fp2 = DeviceFingerprint::new()
            .with_ssdp_server("Samsung UPnP")
            .with_open_ports([8002]);

        fp1.merge(fp2);
        assert_eq!(fp1.ssdp_server.as_deref(), Some("Samsung UPnP"));
        assert_eq!(fp1.open_ports, vec![8001, 8002]);
    }
}
