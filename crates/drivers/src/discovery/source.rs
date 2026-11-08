//! Unified [`DiscoverySource`] trait and protocol implementations.
//!
//! Provides a standardized async discovery trait and concrete implementations for:
//! - SSDP / UPnP discovery
//! - Wake-on-LAN and TCP power probe discovery
//! - Samsung Tizen discovery
//! - LG webOS discovery
//! - Roku ECP discovery
//!
//! Includes a [`CompositeDiscoverySource`] to compose multiple sources without
//! touching orchestrator logic, enabling open extensibility for new vendors.

use std::fmt::Debug;
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use crate::discovery::client::SsdpScanner;
use crate::discovery::error::Result;
use crate::discovery::fingerprint::DeviceFingerprint;
use crate::discovery::probe::{
    probe_host, ProbeSignal, ProbedVendor, ROKU_ECP_PORT, TIZEN_API_PORT, WEBOS_SSAP_PORT,
};
use crate::discovery::types::{DiscoveredDeviceKind, DiscoveryMethod};
use crate::discovery::universal::DiscoveredDevice;

/// An asynchronous discovery source capable of discovering devices.
#[async_trait]
pub trait DiscoverySource: Send + Sync + Debug {
    /// Identifier or name of the discovery source.
    fn name(&self) -> &'static str;

    /// Primary discovery mechanism employed by this source.
    fn method(&self) -> DiscoveryMethod;

    /// Executes discovery and yields discovered devices.
    async fn discover(&self) -> Result<Vec<DiscoveredDevice>>;
}

/// SSDP / UPnP discovery source.
#[derive(Debug, Clone, Default)]
pub struct SsdpDiscoverySource {
    scanner: SsdpScanner,
}

impl SsdpDiscoverySource {
    /// Creates a new SSDP discovery source with default scanner options.
    #[must_use]
    pub fn new() -> Self {
        Self {
            scanner: SsdpScanner::new(),
        }
    }

    /// Creates a new SSDP discovery source wrapping the given [`SsdpScanner`].
    #[must_use]
    pub fn with_scanner(scanner: SsdpScanner) -> Self {
        Self { scanner }
    }

    /// Access the underlying [`SsdpScanner`].
    #[must_use]
    pub fn scanner(&self) -> &SsdpScanner {
        &self.scanner
    }
}

#[async_trait]
impl DiscoverySource for SsdpDiscoverySource {
    fn name(&self) -> &'static str {
        "ssdp"
    }

    fn method(&self) -> DiscoveryMethod {
        DiscoveryMethod::Ssdp
    }

    async fn discover(&self) -> Result<Vec<DiscoveredDevice>> {
        let results = self.scanner.discover_tvs().await.unwrap_or_default();
        let mut devices = Vec::new();

        for (resp, desc) in results {
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

            devices.push(DiscoveredDevice {
                id,
                friendly_name,
                ip,
                kind,
                fingerprint,
                discovery_methods: vec![DiscoveryMethod::Ssdp],
            });
        }

        Ok(devices)
    }
}

/// Wake-on-LAN and TCP power probe discovery source.
///
/// Probes configured IP targets or subnets over TCP power check ports (e.g. port 9/WOL, ports 3000/8001/8060).
#[derive(Debug, Clone)]
pub struct WolDiscoverySource {
    targets: Vec<Ipv4Addr>,
    timeout: Duration,
}

impl WolDiscoverySource {
    /// Create a new WOL/power probe discovery source with an empty initial target list.
    #[must_use]
    pub fn new() -> Self {
        Self {
            targets: Vec::new(),
            timeout: Duration::from_millis(300),
        }
    }

    /// Configure targets for the WOL/power probe discovery source.
    #[must_use]
    pub fn with_targets(mut self, targets: impl IntoIterator<Item = Ipv4Addr>) -> Self {
        self.targets = targets.into_iter().collect();
        self
    }

    /// Configure probe timeout.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

impl Default for WolDiscoverySource {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DiscoverySource for WolDiscoverySource {
    fn name(&self) -> &'static str {
        "wol_power_probe"
    }

    fn method(&self) -> DiscoveryMethod {
        DiscoveryMethod::LocalHttp
    }

    async fn discover(&self) -> Result<Vec<DiscoveredDevice>> {
        let mut devices = Vec::new();

        for ip in &self.targets {
            if let Ok(Some(probed)) = probe_host(std::net::IpAddr::V4(*ip), self.timeout).await {
                let id = format!("probed-wol-{}", ip);
                let (friendly_name, kind, ports) = match probed.signal {
                    ProbeSignal::Awake { vendor, port } => {
                        let kind = match vendor {
                            ProbedVendor::Tizen { model, .. } => DiscoveredDeviceKind::TvBrand(
                                model.unwrap_or_else(|| "Samsung Tizen TV".to_string()),
                            ),
                            ProbedVendor::WebOs => {
                                DiscoveredDeviceKind::TvBrand("LG webOS TV".to_string())
                            }
                            ProbedVendor::Roku { model } => DiscoveredDeviceKind::TvBrand(
                                model.unwrap_or_else(|| "Roku TV".to_string()),
                            ),
                            ProbedVendor::Atr => {
                                DiscoveredDeviceKind::AndroidBox("Android TV".to_string())
                            }
                        };
                        (format!("Awake Display ({ip})"), kind, vec![port])
                    }
                    ProbeSignal::Standby { ports_refused } => (
                        format!("Standby Display ({ip})"),
                        DiscoveredDeviceKind::Unknown("Standby Display".to_string()),
                        ports_refused,
                    ),
                };

                let fingerprint = DeviceFingerprint::new().with_open_ports(ports);

                devices.push(DiscoveredDevice {
                    id,
                    friendly_name,
                    ip: Some(*ip),
                    kind,
                    fingerprint,
                    discovery_methods: vec![DiscoveryMethod::LocalHttp],
                });
            }
        }

        Ok(devices)
    }
}

/// Samsung Tizen dedicated discovery source (using Samsung API probe & SSDP target).
#[derive(Debug, Clone)]
pub struct SamsungDiscoverySource {
    targets: Vec<Ipv4Addr>,
    timeout: Duration,
}

impl SamsungDiscoverySource {
    /// Creates a new Samsung discovery source.
    #[must_use]
    pub fn new() -> Self {
        Self {
            targets: Vec::new(),
            timeout: Duration::from_millis(300),
        }
    }

    /// Add targets to probe for Samsung Tizen.
    #[must_use]
    pub fn with_targets(mut self, targets: impl IntoIterator<Item = Ipv4Addr>) -> Self {
        self.targets = targets.into_iter().collect();
        self
    }

    /// Set probe timeout.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

impl Default for SamsungDiscoverySource {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DiscoverySource for SamsungDiscoverySource {
    fn name(&self) -> &'static str {
        "samsung"
    }

    fn method(&self) -> DiscoveryMethod {
        DiscoveryMethod::LocalHttp
    }

    async fn discover(&self) -> Result<Vec<DiscoveredDevice>> {
        let mut devices = Vec::new();

        for ip in &self.targets {
            if let Ok(Some(probed)) = probe_host(std::net::IpAddr::V4(*ip), self.timeout).await {
                if let ProbeSignal::Awake {
                    vendor: ProbedVendor::Tizen { model, wifi_mac },
                    port: TIZEN_API_PORT,
                } = probed.signal
                {
                    let friendly_name = model
                        .clone()
                        .unwrap_or_else(|| format!("Samsung Smart TV ({ip})"));
                    let mut fp = DeviceFingerprint::new().with_open_ports([TIZEN_API_PORT]);
                    if let Some(mac) = &wifi_mac {
                        fp = fp.with_mac_pattern(mac);
                    }

                    devices.push(DiscoveredDevice {
                        id: format!("samsung-{ip}"),
                        friendly_name,
                        ip: Some(*ip),
                        kind: DiscoveredDeviceKind::TvBrand(
                            model.unwrap_or_else(|| "Samsung Tizen TV".to_string()),
                        ),
                        fingerprint: fp,
                        discovery_methods: vec![DiscoveryMethod::LocalHttp],
                    });
                }
            }
        }

        Ok(devices)
    }
}

/// LG webOS dedicated discovery source (using SSAP port probe).
#[derive(Debug, Clone)]
pub struct LgDiscoverySource {
    targets: Vec<Ipv4Addr>,
    timeout: Duration,
}

impl LgDiscoverySource {
    /// Creates a new LG webOS discovery source.
    #[must_use]
    pub fn new() -> Self {
        Self {
            targets: Vec::new(),
            timeout: Duration::from_millis(300),
        }
    }

    /// Add targets to probe for LG webOS.
    #[must_use]
    pub fn with_targets(mut self, targets: impl IntoIterator<Item = Ipv4Addr>) -> Self {
        self.targets = targets.into_iter().collect();
        self
    }

    /// Set probe timeout.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

impl Default for LgDiscoverySource {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DiscoverySource for LgDiscoverySource {
    fn name(&self) -> &'static str {
        "lg"
    }

    fn method(&self) -> DiscoveryMethod {
        DiscoveryMethod::LocalHttp
    }

    async fn discover(&self) -> Result<Vec<DiscoveredDevice>> {
        let mut devices = Vec::new();

        for ip in &self.targets {
            if let Ok(Some(probed)) = probe_host(std::net::IpAddr::V4(*ip), self.timeout).await {
                if let ProbeSignal::Awake {
                    vendor: ProbedVendor::WebOs,
                    port: WEBOS_SSAP_PORT,
                } = probed.signal
                {
                    let friendly_name = format!("LG webOS TV ({ip})");
                    let fp = DeviceFingerprint::new().with_open_ports([WEBOS_SSAP_PORT]);

                    devices.push(DiscoveredDevice {
                        id: format!("lg-{ip}"),
                        friendly_name,
                        ip: Some(*ip),
                        kind: DiscoveredDeviceKind::TvBrand("LG webOS TV".to_string()),
                        fingerprint: fp,
                        discovery_methods: vec![DiscoveryMethod::LocalHttp],
                    });
                }
            }
        }

        Ok(devices)
    }
}

/// Roku dedicated discovery source (using ECP device-info probe).
#[derive(Debug, Clone)]
pub struct RokuDiscoverySource {
    targets: Vec<Ipv4Addr>,
    timeout: Duration,
}

impl RokuDiscoverySource {
    /// Creates a new Roku discovery source.
    #[must_use]
    pub fn new() -> Self {
        Self {
            targets: Vec::new(),
            timeout: Duration::from_millis(300),
        }
    }

    /// Add targets to probe for Roku ECP.
    #[must_use]
    pub fn with_targets(mut self, targets: impl IntoIterator<Item = Ipv4Addr>) -> Self {
        self.targets = targets.into_iter().collect();
        self
    }

    /// Set probe timeout.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

impl Default for RokuDiscoverySource {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DiscoverySource for RokuDiscoverySource {
    fn name(&self) -> &'static str {
        "roku"
    }

    fn method(&self) -> DiscoveryMethod {
        DiscoveryMethod::LocalHttp
    }

    async fn discover(&self) -> Result<Vec<DiscoveredDevice>> {
        let mut devices = Vec::new();

        for ip in &self.targets {
            if let Ok(Some(probed)) = probe_host(std::net::IpAddr::V4(*ip), self.timeout).await {
                if let ProbeSignal::Awake {
                    vendor: ProbedVendor::Roku { model },
                    port: ROKU_ECP_PORT,
                } = probed.signal
                {
                    let friendly_name = model
                        .clone()
                        .unwrap_or_else(|| format!("Roku Streaming Player ({ip})"));
                    let fp = DeviceFingerprint::new().with_open_ports([ROKU_ECP_PORT]);

                    devices.push(DiscoveredDevice {
                        id: format!("roku-{ip}"),
                        friendly_name,
                        ip: Some(*ip),
                        kind: DiscoveredDeviceKind::TvBrand(
                            model.unwrap_or_else(|| "Roku TV".to_string()),
                        ),
                        fingerprint: fp,
                        discovery_methods: vec![DiscoveryMethod::LocalHttp],
                    });
                }
            }
        }

        Ok(devices)
    }
}

/// Composite discovery source that combines multiple arbitrary [`DiscoverySource`] implementations.
///
/// Merges results across sources by IP / device ID, allowing new vendors and protocols
/// to be added without touching the orchestrator or facade.
#[derive(Default)]
pub struct CompositeDiscoverySource {
    sources: Vec<Arc<dyn DiscoverySource>>,
}

impl std::fmt::Debug for CompositeDiscoverySource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompositeDiscoverySource")
            .field(
                "sources",
                &self
                    .sources
                    .iter()
                    .map(|s| s.name())
                    .collect::<Vec<&'static str>>(),
            )
            .finish()
    }
}

impl CompositeDiscoverySource {
    /// Creates a new empty composite discovery source.
    #[must_use]
    pub fn new() -> Self {
        Self {
            sources: Vec::new(),
        }
    }

    /// Adds a discovery source to the composite pipeline.
    pub fn add_source(&mut self, source: Arc<dyn DiscoverySource>) {
        self.sources.push(source);
    }

    /// Builder method to add a discovery source.
    #[must_use]
    pub fn with_source(mut self, source: Arc<dyn DiscoverySource>) -> Self {
        self.add_source(source);
        self
    }

    /// Access the registered discovery sources.
    #[must_use]
    pub fn sources(&self) -> &[Arc<dyn DiscoverySource>] {
        &self.sources
    }

    /// Merges a list of discovered devices, combining fingerprints and discovery methods by IP/ID.
    pub fn merge_devices(
        devices: impl IntoIterator<Item = DiscoveredDevice>,
    ) -> Vec<DiscoveredDevice> {
        let mut merged: Vec<DiscoveredDevice> = Vec::new();

        for dev in devices {
            if let Some(existing) = merged
                .iter_mut()
                .find(|d| (d.ip.is_some() && d.ip == dev.ip) || d.id == dev.id)
            {
                existing.fingerprint.merge(dev.fingerprint);
                for method in dev.discovery_methods {
                    if !existing.discovery_methods.contains(&method) {
                        existing.discovery_methods.push(method);
                    }
                }
                if matches!(existing.kind, DiscoveredDeviceKind::Unknown(_))
                    && !matches!(dev.kind, DiscoveredDeviceKind::Unknown(_))
                {
                    existing.kind = dev.kind;
                    existing.friendly_name = dev.friendly_name;
                }
            } else {
                merged.push(dev);
            }
        }

        merged
    }
}

#[async_trait]
impl DiscoverySource for CompositeDiscoverySource {
    fn name(&self) -> &'static str {
        "composite"
    }

    fn method(&self) -> DiscoveryMethod {
        DiscoveryMethod::LocalHttp
    }

    async fn discover(&self) -> Result<Vec<DiscoveredDevice>> {
        let mut all_discovered = Vec::new();

        for source in &self.sources {
            if let Ok(mut devs) = source.discover().await {
                all_discovered.append(&mut devs);
            }
        }

        Ok(Self::merge_devices(all_discovered))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mock discovery source for testing orchestration.
    #[derive(Debug)]
    struct MockSource {
        source_name: &'static str,
        method: DiscoveryMethod,
        devices: Vec<DiscoveredDevice>,
    }

    #[async_trait]
    impl DiscoverySource for MockSource {
        fn name(&self) -> &'static str {
            self.source_name
        }

        fn method(&self) -> DiscoveryMethod {
            self.method
        }

        async fn discover(&self) -> Result<Vec<DiscoveredDevice>> {
            Ok(self.devices.clone())
        }
    }

    #[tokio::test]
    async fn composite_discovery_source_orchestrates_and_merges() {
        let dev1 = DiscoveredDevice {
            id: "samsung-1".to_string(),
            friendly_name: "Samsung Living Room".to_string(),
            ip: Some(Ipv4Addr::new(192, 168, 1, 10)),
            kind: DiscoveredDeviceKind::TvBrand("Samsung Tizen TV".to_string()),
            fingerprint: DeviceFingerprint::new().with_open_ports([8001]),
            discovery_methods: vec![DiscoveryMethod::LocalHttp],
        };

        let dev2 = DiscoveredDevice {
            id: "lg-1".to_string(),
            friendly_name: "LG Bedroom".to_string(),
            ip: Some(Ipv4Addr::new(192, 168, 1, 20)),
            kind: DiscoveredDeviceKind::TvBrand("LG webOS TV".to_string()),
            fingerprint: DeviceFingerprint::new().with_open_ports([3000]),
            discovery_methods: vec![DiscoveryMethod::LocalHttp],
        };

        // Same IP as dev1, but detected via SSDP with additional fingerprint data
        let dev1_ssdp = DiscoveredDevice {
            id: "uuid:samsung-1".to_string(),
            friendly_name: "Samsung Living Room".to_string(),
            ip: Some(Ipv4Addr::new(192, 168, 1, 10)),
            kind: DiscoveredDeviceKind::TvBrand("Samsung Tizen TV".to_string()),
            fingerprint: DeviceFingerprint::new().with_mdns_name("samsung-tizen"),
            discovery_methods: vec![DiscoveryMethod::Ssdp],
        };

        let source1 = Arc::new(MockSource {
            source_name: "mock_samsung",
            method: DiscoveryMethod::LocalHttp,
            devices: vec![dev1],
        });

        let source2 = Arc::new(MockSource {
            source_name: "mock_lg",
            method: DiscoveryMethod::LocalHttp,
            devices: vec![dev2],
        });

        let source3 = Arc::new(MockSource {
            source_name: "mock_ssdp",
            method: DiscoveryMethod::Ssdp,
            devices: vec![dev1_ssdp],
        });

        let composite = CompositeDiscoverySource::new()
            .with_source(source1)
            .with_source(source2)
            .with_source(source3);

        assert_eq!(composite.sources().len(), 3);
        assert_eq!(composite.name(), "composite");

        let results = composite.discover().await.expect("discovery succeeds");
        assert_eq!(results.len(), 2, "Duplicate IP 192.168.1.10 must be merged");

        let merged_samsung = results
            .iter()
            .find(|d| d.ip == Some(Ipv4Addr::new(192, 168, 1, 10)))
            .expect("Samsung must be present");
        assert!(merged_samsung
            .discovery_methods
            .contains(&DiscoveryMethod::LocalHttp));
        assert!(merged_samsung
            .discovery_methods
            .contains(&DiscoveryMethod::Ssdp));
        assert_eq!(merged_samsung.fingerprint.open_ports, vec![8001]);
        assert_eq!(
            merged_samsung.fingerprint.mdns_name.as_deref(),
            Some("samsung-tizen")
        );

        let lg = results
            .iter()
            .find(|d| d.ip == Some(Ipv4Addr::new(192, 168, 1, 20)))
            .expect("LG must be present");
        assert_eq!(lg.friendly_name, "LG Bedroom");
    }

    #[test]
    fn default_constructors_for_discovery_sources() {
        let ssdp = SsdpDiscoverySource::new();
        assert_eq!(ssdp.name(), "ssdp");
        assert_eq!(ssdp.method(), DiscoveryMethod::Ssdp);

        let wol = WolDiscoverySource::new().with_targets([Ipv4Addr::new(10, 0, 0, 1)]);
        assert_eq!(wol.name(), "wol_power_probe");
        assert_eq!(wol.method(), DiscoveryMethod::LocalHttp);

        let samsung = SamsungDiscoverySource::new().with_targets([Ipv4Addr::new(10, 0, 0, 2)]);
        assert_eq!(samsung.name(), "samsung");
        assert_eq!(samsung.method(), DiscoveryMethod::LocalHttp);

        let lg = LgDiscoverySource::new().with_targets([Ipv4Addr::new(10, 0, 0, 3)]);
        assert_eq!(lg.name(), "lg");
        assert_eq!(lg.method(), DiscoveryMethod::LocalHttp);

        let roku = RokuDiscoverySource::new().with_targets([Ipv4Addr::new(10, 0, 0, 4)]);
        assert_eq!(roku.name(), "roku");
        assert_eq!(roku.method(), DiscoveryMethod::LocalHttp);
    }
}
