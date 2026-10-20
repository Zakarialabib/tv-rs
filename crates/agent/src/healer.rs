//! The healing ladder: what to do when a device keeps failing.
//!
//! The point of the ladder is arithmetic, not cleverness. A device that fails once
//! gets another chance; a device that fails the same way N times is taken out of
//! rotation before it can turn one access-point reboot into a fleet-wide reconnect
//! storm.

use std::collections::HashMap;

use tv_rs_core::{DeviceId, Remedy, TvError};

/// What the agent should do about a failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealerAction {
    /// Power-on attempt (Wake-on-LAN) before anything else: an unreachable TV is
    /// usually a sleeping one.
    Wake,
    /// Try the connection again with backoff.
    Reconnect,
    /// The credential is gone; a human has to re-pair. Retrying will not help.
    Reprovision,
    /// Stop touching the device, and report it.
    Quarantine,
}

/// Counts consecutive failures per device and answers with a ladder step.
#[derive(Debug, Clone)]
pub struct HealerLadder {
    failures: HashMap<DeviceId, u32>,
    max_attempts: u32,
}

impl Default for HealerLadder {
    fn default() -> Self {
        Self::new(3)
    }
}

impl HealerLadder {
    /// A ladder that quarantines after `max_attempts` consecutive failures.
    #[must_use]
    pub fn new(max_attempts: u32) -> Self {
        Self {
            failures: HashMap::new(),
            max_attempts: max_attempts.max(1),
        }
    }

    /// Records a failure and returns the next step.
    ///
    /// The mapping is by [`Remedy`], not by error type: that is the whole reason the
    /// taxonomy exists.
    pub fn on_failure(&mut self, device: &DeviceId, error: &TvError) -> HealerAction {
        match error.remedy() {
            // Retrying a missing credential is how a fleet gets rate-limited.
            Remedy::Reprovision => HealerAction::Reprovision,
            // A capability that does not exist will not start existing.
            Remedy::Disable | Remedy::Quarantine => {
                self.failures.insert(device.clone(), self.max_attempts);
                HealerAction::Quarantine
            }
            Remedy::ApplyQuirk => HealerAction::Reconnect,
            Remedy::Wait | Remedy::BackOff | Remedy::Retry => {
                let count = self.failures.entry(device.clone()).or_insert(0);
                *count += 1;
                if *count >= self.max_attempts {
                    HealerAction::Quarantine
                } else if *count == 1 {
                    // First failure on an unreachable TV: try the cheapest fix.
                    HealerAction::Wake
                } else {
                    HealerAction::Reconnect
                }
            }
        }
    }

    /// A success resets the device's counter.
    pub fn on_success(&mut self, device: &DeviceId) {
        self.failures.remove(device);
    }

    /// Consecutive failures recorded for a device.
    #[must_use]
    pub fn failures(&self, device: &DeviceId) -> u32 {
        self.failures.get(device).copied().unwrap_or(0)
    }

    /// Whether a device is currently quarantined.
    #[must_use]
    pub fn is_quarantined(&self, device: &DeviceId) -> bool {
        self.failures(device) >= self.max_attempts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tv_rs_core::{AuthError, Capability, TransportError};

    fn device() -> DeviceId {
        DeviceId::new("tv-1").unwrap()
    }

    #[test]
    fn the_first_failure_wakes_before_reconnecting() {
        let mut ladder = HealerLadder::new(3);
        let action = ladder.on_failure(&device(), &TvError::Transport(TransportError::Timeout));
        assert_eq!(action, HealerAction::Wake);
        assert_eq!(ladder.failures(&device()), 1);
    }

    #[test]
    fn repeated_failures_escalate_and_then_quarantine() {
        let mut ladder = HealerLadder::new(3);
        let error = TvError::Transport(TransportError::Unreachable("10.0.0.5".into()));
        assert_eq!(ladder.on_failure(&device(), &error), HealerAction::Wake);
        assert_eq!(
            ladder.on_failure(&device(), &error),
            HealerAction::Reconnect
        );
        assert_eq!(
            ladder.on_failure(&device(), &error),
            HealerAction::Quarantine
        );
        assert!(ladder.is_quarantined(&device()));
    }

    #[test]
    fn a_success_clears_the_history() {
        let mut ladder = HealerLadder::new(2);
        let error = TvError::Transport(TransportError::Timeout);
        ladder.on_failure(&device(), &error);
        ladder.on_success(&device());
        assert_eq!(ladder.failures(&device()), 0);
        assert_eq!(ladder.on_failure(&device(), &error), HealerAction::Wake);
    }

    #[test]
    fn missing_credentials_are_never_retried() {
        let mut ladder = HealerLadder::new(3);
        let action = ladder.on_failure(&device(), &TvError::Auth(AuthError::PairingRequired));
        assert_eq!(action, HealerAction::Reprovision);
    }

    #[test]
    fn a_missing_capability_quarantines_immediately() {
        let mut ladder = HealerLadder::new(3);
        let action = ladder.on_failure(&device(), &TvError::Unsupported(Capability::Wake));
        assert_eq!(action, HealerAction::Quarantine);
    }
}
