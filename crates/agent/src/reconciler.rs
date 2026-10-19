//! The reconcile decision, as a pure function.
//!
//! Everything here is deliberately synchronous and side-effect free: given what was
//! asked for, what is believed, and what the device can do, it answers with one
//! action. That makes the interesting part of an agent testable without a network, a
//! clock or a runtime — and it is why the 50-device simulation is a loop, not a
//! fixture.

use tv_rs_core::{AppRef, Capability, CapabilitySet, Confidence, ObservedState, PowerState};

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
}

/// Decides the next action for a device.
///
/// The rules are ordered by what the least information can support: a stale belief
/// is always probed before it is acted on, and an impossible request is refused
/// before a device is touched at all.
#[must_use]
pub fn decide(
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
    use tv_rs_core::ObservedField;

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
}
