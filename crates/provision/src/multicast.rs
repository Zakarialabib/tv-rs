//! Multicast lock and network permissions gating.
//!
//! Enforces operating-system specific multicast constraints:
//! - iOS: Requires the `com.apple.developer.networking.multicast` entitlement to receive UDP multicast.
//! - Android: Requires acquiring `android.net.wifi.WifiManager.MulticastLock` to receive mDNS / SSDP packets.
//! - Filtered Network Fallback: If multicast is blocked or filtered (or lock cannot be acquired),
//!   the client fails over to direct QR / unicast hostd connection.

use crate::error::{ProvisionError, Result};

/// Target operating system for companion runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MobilePlatform {
    /// Apple iOS.
    Ios,
    /// Google Android.
    Android,
    /// Desktop or other platforms without multicast restrictions.
    Generic,
}

/// Multicast status and permissions guardian.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MulticastLockGuard {
    platform: MobilePlatform,
    has_ios_entitlement: bool,
    android_lock_acquired: bool,
}

impl MulticastLockGuard {
    /// Create a new guard for a platform.
    pub fn new(platform: MobilePlatform) -> Self {
        Self {
            platform,
            has_ios_entitlement: false,
            android_lock_acquired: false,
        }
    }

    /// Set iOS multicast entitlement state.
    pub fn with_ios_entitlement(mut self, granted: bool) -> Self {
        self.has_ios_entitlement = granted;
        self
    }

    /// Set Android multicast lock state.
    pub fn with_android_lock(mut self, acquired: bool) -> Self {
        self.android_lock_acquired = acquired;
        self
    }

    /// Check if local network multicast (mDNS / SSDP) is permitted.
    ///
    /// # Errors
    ///
    /// Returns [`ProvisionError::MulticastPermissionRequired`] when:
    /// - On iOS and `com.apple.developer.networking.multicast` entitlement is missing.
    /// - On Android and `MulticastLock` is not held.
    pub fn verify_multicast_allowed(&self) -> Result<()> {
        match self.platform {
            MobilePlatform::Ios => {
                if !self.has_ios_entitlement {
                    return Err(ProvisionError::MulticastPermissionRequired(
                        "iOS requires com.apple.developer.networking.multicast entitlement for mDNS/SSDP".into(),
                    ));
                }
            }
            MobilePlatform::Android => {
                if !self.android_lock_acquired {
                    return Err(ProvisionError::MulticastPermissionRequired(
                        "Android WifiManager.MulticastLock must be acquired before discovery"
                            .into(),
                    ));
                }
            }
            MobilePlatform::Generic => {}
        }
        Ok(())
    }

    /// Whether to fall back to direct unicast or QR pairing when multicast is unavailable.
    pub fn should_fallback_to_direct_pairing(&self) -> bool {
        self.verify_multicast_allowed().is_err()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ios_entitlement_check() {
        let guard_no_entitlement = MulticastLockGuard::new(MobilePlatform::Ios);
        assert!(guard_no_entitlement.verify_multicast_allowed().is_err());
        assert!(guard_no_entitlement.should_fallback_to_direct_pairing());

        let guard_with_entitlement =
            MulticastLockGuard::new(MobilePlatform::Ios).with_ios_entitlement(true);
        assert!(guard_with_entitlement.verify_multicast_allowed().is_ok());
        assert!(!guard_with_entitlement.should_fallback_to_direct_pairing());
    }

    #[test]
    fn test_android_multicast_lock_check() {
        let guard_no_lock = MulticastLockGuard::new(MobilePlatform::Android);
        assert!(guard_no_lock.verify_multicast_allowed().is_err());

        let guard_locked = MulticastLockGuard::new(MobilePlatform::Android).with_android_lock(true);
        assert!(guard_locked.verify_multicast_allowed().is_ok());
    }

    #[test]
    fn test_generic_platform() {
        let guard = MulticastLockGuard::new(MobilePlatform::Generic);
        assert!(guard.verify_multicast_allowed().is_ok());
        assert!(!guard.should_fallback_to_direct_pairing());
    }
}
