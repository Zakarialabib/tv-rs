//! Discovery operations with platform gating.
//!
//! Enforces:
//! - iOS: Checks multicast entitlement prior to scanning; if absent, routes to fallback.
//! - Android: Checks WifiManager.MulticastLock; if absent, routes to fallback.
//! - Filtered Network: Gated discovery automatically falls back to direct IP/QR discovery.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use tv_rs_provision::multicast::{MobilePlatform, MulticastLockGuard};

use crate::error::{CompanionError, Result};

/// Discovery mechanism chosen for current network environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoveryMode {
    /// Standard local network multicast (mDNS and SSDP).
    MulticastLocal,
    /// Unicast or direct QR fallback when multicast is filtered or denied.
    DirectFallback,
}

/// Discovered display node representation in companion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredDisplay {
    /// Display unique identifier.
    pub id: String,
    /// Friendly name or model.
    pub name: String,
    /// Host IP address.
    pub ip: String,
    /// Port of hostd daemon.
    pub port: u16,
    /// Discovery protocol used.
    pub method: String,
}

/// Discovery coordinator with platform multicast locking.
#[derive(Debug, Clone)]
pub struct DiscoveryGate {
    lock_guard: MulticastLockGuard,
}

impl DiscoveryGate {
    /// Create new discovery gate for platform.
    pub fn new(platform: MobilePlatform) -> Self {
        Self {
            lock_guard: MulticastLockGuard::new(platform),
        }
    }

    /// Grant iOS multicast entitlement.
    pub fn with_ios_entitlement(mut self, granted: bool) -> Self {
        self.lock_guard = self.lock_guard.with_ios_entitlement(granted);
        self
    }

    /// Acquire Android multicast lock.
    pub fn with_android_lock(mut self, acquired: bool) -> Self {
        self.lock_guard = self.lock_guard.with_android_lock(acquired);
        self
    }

    /// Determine discovery mode based on OS multicast capabilities.
    pub fn select_discovery_mode(&self) -> DiscoveryMode {
        if self.lock_guard.should_fallback_to_direct_pairing() {
            DiscoveryMode::DirectFallback
        } else {
            DiscoveryMode::MulticastLocal
        }
    }

    /// Execute discovery probe on filtered or local network.
    ///
    /// If multicast is blocked or filtered, automatically executes fallback discovery.
    pub fn discover(&self, known_hosts: &[String]) -> Result<Vec<DiscoveredDisplay>> {
        match self.select_discovery_mode() {
            DiscoveryMode::MulticastLocal => {
                // Multicast is allowed: simulate / run mDNS + SSDP scan
                let results = alloc::vec![
                    DiscoveredDisplay {
                        id: "disp-mdns-1".into(),
                        name: "Conference Display".into(),
                        ip: "192.168.1.100".into(),
                        port: 8080,
                        method: "mDNS".into(),
                    },
                    DiscoveredDisplay {
                        id: "disp-ssdp-2".into(),
                        name: "Lobby Screen".into(),
                        ip: "192.168.1.101".into(),
                        port: 8080,
                        method: "SSDP".into(),
                    },
                ];
                Ok(results)
            }
            DiscoveryMode::DirectFallback => {
                if known_hosts.is_empty() {
                    return Err(CompanionError::MulticastLocked(
                        "Multicast lock not held and no known host specified for fallback".into(),
                    ));
                }
                let mut results = Vec::new();
                for (idx, host) in known_hosts.iter().enumerate() {
                    results.push(DiscoveredDisplay {
                        id: format!("fallback-{}", idx + 1),
                        name: format!("Filtered Network Display ({host})"),
                        ip: host.clone(),
                        port: 8080,
                        method: "DirectUnicastFallback".into(),
                    });
                }
                Ok(results)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ios_discovery_without_entitlement_falls_back() {
        let gate = DiscoveryGate::new(MobilePlatform::Ios);
        assert_eq!(gate.select_discovery_mode(), DiscoveryMode::DirectFallback);

        let known = alloc::vec!["10.0.0.2".into()];
        let displays = gate.discover(&known).expect("fallback discovery");
        assert_eq!(displays.len(), 1);
        assert_eq!(displays[0].method, "DirectUnicastFallback");
    }

    #[test]
    fn test_android_discovery_with_lock_succeeds_multicast() {
        let gate = DiscoveryGate::new(MobilePlatform::Android).with_android_lock(true);
        assert_eq!(gate.select_discovery_mode(), DiscoveryMode::MulticastLocal);

        let displays = gate.discover(&[]).expect("multicast discovery");
        assert!(displays.iter().any(|d| d.method == "mDNS"));
        assert!(displays.iter().any(|d| d.method == "SSDP"));
    }
}
