//! Leased control: who may touch a device, until when, and how.
//!
//! A `MaintenanceLease` is the temporary hand-over of one device's control
//! surface to a second-party holder — a technician's tablet driving the remote
//! UI, a commissioning rig pairing a fresh panel. While a lease is active, the
//! fleet agent must not fight the holder for the wire: [`LeaseMode`] names the
//! two shapes that takes.
//!
//! Everything here is a timestamped fact, not a timer: `is_active` takes `now`
//! from the caller, the same way [`crate::state`] takes confidence. The crate
//! keeps no clock — the agent runner and the HTTP surface each supply theirs.

use crate::descriptor::DeviceId;

/// How a lease holder expects the control plane to behave while they hold it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum LeaseMode {
    /// The holder drives the device; the fleet observes and relays only.
    ///
    /// The agent suppresses every mutation (reconciliation, telemetry remedies,
    /// heartbeat writes it would leave behind) but keeps reading state and
    /// auditing it, and forwards the holder's own [`crate::imperative::
    /// ImperativeCommand`] traffic through the warm socket. The panel never
    /// goes dark for the fleet — it is borrowed, not mine.
    ObserveAndRelay,
    /// The holder owns the connection; the agent lets go completely.
    ///
    /// Pooled sockets are dropped (a pairing solemnity or a display-id wall
    /// cannot survive a second client), desired-state reconciliation stops, and
    /// the device yields until the lease expires. The agent also holds its own
    /// writes, including the power probe it would normally refresh, because the
    /// holder's session may require being the only client.
    DisconnectAndYield,
}

impl LeaseMode {
    /// A stable `snake_case` label for logs and audit entries.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ObserveAndRelay => "observe_and_relay",
            Self::DisconnectAndYield => "disconnect_and_yield",
        }
    }
}

/// A temporary grant of one device's control surface.
///
/// One lease per device: the type does not model stackings. Acquiring against an
/// active lease is the caller's decision, and they get the existing lease back
/// to reason about — this type stays a fact, not a registry.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MaintenanceLease {
    /// Who holds it. A stable client identity (the same string a backend uses
    /// as its pairing identity), not a session id that rotates mid-lease.
    pub holder_id: String,
    /// The device being borrowed.
    pub device_id: DeviceId,
    /// Expiry instant, Unix milliseconds. `is_active` compares against what the
    /// caller says `now` is.
    pub expires_at_unix_ms: u64,
    /// How aggressive the holder wants the agent's coexistence to be.
    pub mode: LeaseMode,
}

impl MaintenanceLease {
    /// Whether the lease still grants control at instant `now_unix_ms`.
    ///
    /// Expiry is inclusive: a lease active at `expires_at_unix_ms` is still
    /// `true`, so a caller renewing on the boundary never races the agent.
    #[must_use]
    pub fn is_active(&self, now_unix_ms: u64) -> bool {
        now_unix_ms <= self.expires_at_unix_ms
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lease(expires_at_unix_ms: u64, mode: LeaseMode) -> MaintenanceLease {
        MaintenanceLease {
            holder_id: "tablet-9".to_string(),
            device_id: DeviceId::new("udn:abc").unwrap(),
            expires_at_unix_ms,
            mode,
        }
    }

    #[test]
    fn expiry_is_inclusive_so_a_boundary_renewal_does_not_race() {
        let active = lease(1_000, LeaseMode::ObserveAndRelay);
        assert!(active.is_active(999));
        assert!(active.is_active(1_000));
        assert!(!active.is_active(1_001));
    }

    #[test]
    fn modes_have_stable_labels() {
        assert_eq!(LeaseMode::ObserveAndRelay.as_str(), "observe_and_relay");
        assert_eq!(
            LeaseMode::DisconnectAndYield.as_str(),
            "disconnect_and_yield"
        );
    }

    #[test]
    #[cfg(feature = "serde")]
    fn serialization_round_trips() {
        let lease = lease(5_000, LeaseMode::DisconnectAndYield);
        let json = serde_json::to_string(&lease).unwrap();
        let parsed: MaintenanceLease = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, lease);
    }
}
