//! The reconcile decision, as a pure function.
//!
//! Everything here is deliberately synchronous and side-effect free: given what was
//! asked for, what is believed, and what the device can do, it answers with one
//! action. That makes the interesting part of an agent testable without a network, a
//! clock or a runtime — and it is why the 50-device simulation is a loop, not a
//! fixture.

use tv_rs_core::{
    AppRef, Capability, CapabilitySet, Confidence, LeaseMode, MaintenanceLease, ObservedState,
    PowerState,
};

use crate::desired::{DesiredPower, DesiredState};

/// What to do next for one device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Nothing needs to happen.
    Nothing,
    /// Read the power state before deciding: the belief is stale or missing.
    ProbePower,
    /// Wake the device, then re-read.
    Wake,
    /// Put the device into standby.
    Sleep,
    /// Bring an application to the front.
    Launch(AppRef),
    /// Stop touching this device and say why.
    Quarantine(&'static str),
    /// A leased device asks the agent to keep its hands off the mutations.
    ///
    /// Under a [`LeaseMode::ObserveAndRelay`] lease the holder drives and the
    /// fleet only watches: wake/sleep/launch decisions are suppressed until
    /// the lease expires, while reads (ProbePower, Nothing) still run so the
    /// dashboard stays truthful. The holder's own [`ImperativeCommand`]
    /// traffic is routed through the relay, not through reconciliation.
    ///
    /// [`ImperativeCommand`]: tv_rs_core::ImperativeCommand
    InhibitedByLease {
        /// Who holds the lease, for the audit line.
        holder_id: String,
    },
    /// A leased device takes the connection with it: the agent yields.
    ///
    /// Under a [`LeaseMode::DisconnectAndYield`] lease the holder owns the
    /// connection — a pairing solemnity or a display-id wall cannot survive a
    /// second client — so reconciliation stops entirely, *including* the
    /// power probe the agent would normally refresh, until the lease expires.
    YieldTransport,
}

/// Decides the next action for a device.
///
/// The three-argument entry point callers without a lease book use: it never
/// inhibits. Lease-aware callers use [`decide_with_lease`], which wraps these
/// same rules.
#[must_use]
pub fn decide(
    desired: &DesiredState,
    observed: &ObservedState,
    capabilities: CapabilitySet,
) -> Decision {
    decide_with_lease(desired, observed, capabilities, None, 0)
}

/// Decides the next action for a device, honouring an active control lease.
///
/// The rules are ordered by what the least information can support: a stale belief
/// is always probed before it is acted on, and an impossible request is refused
/// before a device is touched at all. An *active* lease — one whose
/// `device_id` matches the device the caller is reconciling, filtered by the
/// caller — is consulted last, so it reshapes the mutating decisions:
///
/// | Lease | `Wake`/`Sleep`/`Launch` | `ProbePower` | `Quarantine` |
/// |---|---|---|---|
/// | [`LeaseMode::ObserveAndRelay`] | [`Decision::InhibitedByLease`] | runs (a read) | runs (a stop, not a write) |
/// | [`LeaseMode::DisconnectAndYield`] | [`Decision::YieldTransport`] | [`Decision::YieldTransport`] | runs |
///
/// An *expired* lease (`is_active(now)` false) falls through to the normal
/// rules: resuming after a lease is the whole point of expiry.
#[must_use]
pub fn decide_with_lease(
    desired: &DesiredState,
    observed: &ObservedState,
    capabilities: CapabilitySet,
    active_lease: Option<&MaintenanceLease>,
    now_unix_ms: u64,
) -> Decision {
    let decision = decide_unleased(desired, observed, capabilities);

    let Some(lease) = active_lease.filter(|lease| lease.is_active(now_unix_ms)) else {
        return decision;
    };

    match lease.mode {
        LeaseMode::ObserveAndRelay => match decision {
            Decision::Wake | Decision::Sleep | Decision::Launch(_) => Decision::InhibitedByLease {
                holder_id: lease.holder_id.clone(),
            },
            Decision::ProbePower | Decision::Nothing | Decision::Quarantine(_) => decision,
            // The decision being reshaped is always the pre-lease one; these
            // two arms cannot occur here and only keep the match exhaustive.
            Decision::InhibitedByLease { .. } | Decision::YieldTransport => decision,
        },
        // The holder owns the connection; even the power probe the agent would
        // normally refresh is its own write.
        LeaseMode::DisconnectAndYield => match decision {
            Decision::Quarantine(_) => decision,
            _ => Decision::YieldTransport,
        },
    }
}

/// The original three-argument rules, before lease reshaping.
fn decide_unleased(
    desired: &DesiredState,
    observed: &ObservedState,
    capabilities: CapabilitySet,
) -> Decision {
    let app = desired.app_ref();

    if app.is_some() && !capabilities.contains(Capability::AppLaunch) {
        return Decision::Quarantine("device cannot launch applications");
    }

    let power = observed.power.confidence;
    let state = observed.power.value;

    // Act on power first: an app cannot be launched onto a sleeping panel.
    match desired.power {
        DesiredPower::On => match (power, state) {
            (Confidence::High, Some(PowerState::On)) => {}
            (Confidence::High, Some(PowerState::Standby | PowerState::Off)) => {
                if !capabilities.contains(Capability::Wake) {
                    return Decision::Quarantine("device is asleep and cannot be woken");
                }
                return Decision::Wake;
            }
            (Confidence::High, Some(PowerState::Unknown)) => {
                return Decision::Quarantine("device cannot report its power state")
            }
            // Unknown or inferred: ask before acting. Sending `Power` to a device
            // that is already on turns it off.
            _ => return Decision::ProbePower,
        },
        DesiredPower::Off => match (power, state) {
            (Confidence::High, Some(PowerState::Standby | PowerState::Off)) => {
                return Decision::Nothing
            }
            (Confidence::High, Some(PowerState::On)) => return Decision::Sleep,
            _ => return Decision::ProbePower,
        },
    }

    // Power is known-on; only the foreground app can be wrong now.
    if let Some(app) = app {
        match observed.foreground.confidence {
            Confidence::High => {
                let current = observed
                    .foreground
                    .value
                    .as_ref()
                    .map(|app| app.id.as_str());
                if current == Some(app.id.as_str()) {
                    Decision::Nothing
                } else {
                    Decision::Launch(app)
                }
            }
            // Not knowing what is in front is not a reason to do nothing: the
            // device would never converge from a cold handle. Launching the desired
            // app when it is already in front is a no-op on every vendor, so the
            // cost of being wrong is one request.
            _ => Decision::Launch(app),
        }
    } else {
        Decision::Nothing
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tv_rs_core::{DeviceId, ObservedField};

    fn capable() -> CapabilitySet {
        CapabilitySet::empty()
            .with(Capability::PowerState)
            .with(Capability::Wake)
            .with(Capability::AppLaunch)
            .with(Capability::Foreground)
    }

    fn on(app: Option<&str>) -> ObservedState {
        ObservedState {
            power: ObservedField::from_read(PowerState::On),
            foreground: match app {
                Some(app) => ObservedField::from_read(AppRef::new(app)),
                None => ObservedField::unknown(),
            },
            ..ObservedState::nothing_observed()
        }
    }

    #[test]
    fn a_device_already_in_the_right_state_is_left_alone() {
        assert_eq!(
            decide(
                &DesiredState::showing("netflix"),
                &on(Some("netflix")),
                capable()
            ),
            Decision::Nothing
        );
    }

    #[test]
    fn the_wrong_app_is_launched() {
        assert_eq!(
            decide(
                &DesiredState::showing("netflix"),
                &on(Some("youtube")),
                capable()
            ),
            Decision::Launch(AppRef::new("netflix"))
        );
    }

    #[test]
    fn stale_power_is_probed_before_anything_is_sent() {
        let observed = ObservedState {
            power: ObservedField::from_unverified_write(PowerState::On),
            ..on(None)
        };
        assert_eq!(
            decide(&DesiredState::showing("netflix"), &observed, capable()),
            Decision::ProbePower,
            "a toggle is not proof; probing costs one request, guessing costs a wrong power command"
        );
    }

    #[test]
    fn a_sleeping_device_with_no_wake_capability_is_quarantined() {
        let observed = ObservedState {
            power: ObservedField::from_read(PowerState::Standby),
            ..ObservedState::nothing_observed()
        };
        let without_wake = CapabilitySet::empty().with(Capability::PowerState);
        assert!(matches!(
            decide(&DesiredState::showing("netflix"), &observed, without_wake),
            Decision::Quarantine(_)
        ));
    }

    #[test]
    fn power_off_sleeps_an_awake_device() {
        assert_eq!(
            decide(
                &DesiredState {
                    power: DesiredPower::Off,
                    app: None
                },
                &on(None),
                capable()
            ),
            Decision::Sleep
        );
    }

    #[test]
    fn an_unknown_foreground_still_launches() {
        // Otherwise a fresh handle never converges: the first pass always has an
        // unknown foreground, and "do nothing" would be permanent.
        let observed = on(None);
        assert_eq!(
            decide(&DesiredState::showing("netflix"), &observed, capable()),
            Decision::Launch(AppRef::new("netflix"))
        );
    }

    #[test]
    fn a_device_that_cannot_launch_is_quarantined_before_anything_is_sent() {
        let without_app = CapabilitySet::empty()
            .with(Capability::PowerState)
            .with(Capability::Wake);
        assert!(matches!(
            decide(&DesiredState::showing("netflix"), &on(None), without_app),
            Decision::Quarantine(_)
        ));
    }

    #[test]
    fn a_stale_power_belief_still_probes_under_an_observe_and_relay_lease() {
        // Observation is half the lease's bargain: the dashboards keep reading
        // while the holder drives.
        let now = 10_000u64;
        let lease = MaintenanceLease {
            holder_id: "tablet-9".to_string(),
            device_id: DeviceId::new("tv-1").unwrap(),
            expires_at_unix_ms: 20_000,
            mode: LeaseMode::ObserveAndRelay,
        };
        let observed = ObservedState {
            power: ObservedField::from_unverified_write(PowerState::On),
            ..on(None)
        };
        assert_eq!(
            decide_with_lease(
                &DesiredState::showing("netflix"),
                &observed,
                capable(),
                Some(&lease),
                now
            ),
            Decision::ProbePower,
            "a toggle is not proof under a lease either"
        );
    }

    #[test]
    fn mutating_decisions_are_inhibited_while_the_holder_drives() {
        let lease = MaintenanceLease {
            holder_id: "tablet-9".to_string(),
            device_id: DeviceId::new("tv-1").unwrap(),
            expires_at_unix_ms: 20_000,
            mode: LeaseMode::ObserveAndRelay,
        };
        let observed = ObservedState {
            power: ObservedField::from_read(PowerState::Standby),
            ..ObservedState::nothing_observed()
        };
        assert_eq!(
            decide_with_lease(
                &DesiredState::showing("netflix"),
                &observed,
                capable(),
                Some(&lease),
                10_000
            ),
            Decision::InhibitedByLease {
                holder_id: "tablet-9".to_string()
            },
            "the agent must not wake the borrowed set out from under the holder"
        );
    }

    #[test]
    fn an_expired_lease_resumes_normal_rules() {
        let lease = MaintenanceLease {
            holder_id: "tablet-9".to_string(),
            device_id: DeviceId::new("tv-1").unwrap(),
            expires_at_unix_ms: 20_000,
            mode: LeaseMode::ObserveAndRelay,
        };
        let observed = ObservedState {
            power: ObservedField::from_read(PowerState::Standby),
            ..ObservedState::nothing_observed()
        };
        assert_eq!(
            decide_with_lease(
                &DesiredState::showing("netflix"),
                &observed,
                capable(),
                Some(&lease),
                20_001
            ),
            Decision::Wake
        );
    }

    #[test]
    fn a_disconnect_and_yield_lease_yields_even_the_power_probe() {
        // The holder may need to be the only client; the agent's own refresh
        // write would announce a second session.
        let lease = MaintenanceLease {
            holder_id: "rig-2".to_string(),
            device_id: DeviceId::new("tv-1").unwrap(),
            expires_at_unix_ms: 20_000,
            mode: LeaseMode::DisconnectAndYield,
        };
        let observed = ObservedState {
            power: ObservedField::from_unverified_write(PowerState::On),
            ..on(None)
        };
        assert_eq!(
            decide_with_lease(
                &DesiredState::showing("netflix"),
                &observed,
                capable(),
                Some(&lease),
                10_000
            ),
            Decision::YieldTransport
        );
    }

    #[test]
    fn a_quarantine_survives_both_lease_modes() {
        // Quarantine is a stop, not a wire write: an impossible request must
        // still be refused while the device is borrowed.
        let without_wake = CapabilitySet::empty().with(Capability::PowerState);
        let observed = ObservedState {
            power: ObservedField::from_read(PowerState::Standby),
            ..ObservedState::nothing_observed()
        };
        for mode in [LeaseMode::ObserveAndRelay, LeaseMode::DisconnectAndYield] {
            let lease = MaintenanceLease {
                holder_id: "tablet-9".to_string(),
                device_id: DeviceId::new("tv-1").unwrap(),
                expires_at_unix_ms: 20_000,
                mode,
            };
            assert!(
                matches!(
                    decide_with_lease(
                        &DesiredState::showing("netflix"),
                        &observed,
                        without_wake,
                        Some(&lease),
                        10_000
                    ),
                    Decision::Quarantine(_)
                ),
                "quarantine survives a {mode:?} lease"
            );
        }
    }
}
