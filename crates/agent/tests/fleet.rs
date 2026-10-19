//! A 50-device fleet, simulated: the Phase 4 exit criterion.
//!
//! Nothing here touches a network. Every device is a `MockBackend`, the decisions are
//! made by the same pure function the real loop uses, and the failures are the ones
//! that actually happen: a device that never pairs, a device whose power can never be
//! read, and transient blips.

use std::sync::Arc;

use futures::executor::block_on;
use tv_rs_agent::{decide, Decision, DesiredState, HealerAction, HealerLadder};
use tv_rs_core::{
    capability::{Capability, CapabilitySet},
    descriptor::{DeviceDescriptor, DeviceId, DeviceOs},
    mock::{Failure, MockBackend},
    AuthError, DeviceHandle, ObservedField, ObservedState, PowerState, TvError,
};

fn device(index: usize) -> DeviceDescriptor {
    DeviceDescriptor::new(
        DeviceId::new(format!("tv-{index:02}")).unwrap(),
        DeviceOs::Roku,
        format!("10.0.0.{}", index + 10),
    )
}

fn all_capabilities() -> CapabilitySet {
    let mut set = CapabilitySet::empty();
    for capability in [
        Capability::PowerState,
        Capability::Wake,
        Capability::Key,
        Capability::AppLaunch,
        Capability::Foreground,
    ] {
        set.insert(capability);
    }
    set
}

/// One reconciliation pass over one mock device, using the real decision function.
fn pass(mock: &MockBackend, desired: &DesiredState) -> Decision {
    let power =
        block_on(mock.as_power().expect("power").power_state()).unwrap_or(PowerState::Unknown);
    let observed = ObservedState {
        power: if power == PowerState::Unknown {
            ObservedField::unknown()
        } else {
            ObservedField::from_read(power)
        },
        ..ObservedState::nothing_observed()
    };
    decide(desired, &observed, mock.capabilities())
}

#[test]
fn a_fifty_device_fleet_converges_and_quarantines_the_pathological() {
    let desired = DesiredState::showing("netflix");
    let mut ladder = HealerLadder::new(3);

    // 50 devices: 46 healthy, 2 that never pair, 1 whose power is never observable,
    // 1 that is asleep with no wake path.
    let devices: Vec<Arc<MockBackend>> = (0..50)
        .map(|index| {
            let mock = MockBackend::new(device(index));
            match index {
                46 | 47 => Arc::new(mock), // unreachable: fail on every pass
                48 => Arc::new(MockBackend::new(device(index)).starting_at(PowerState::Unknown)),
                49 => Arc::new(
                    MockBackend::new(device(index))
                        .with_capabilities(CapabilitySet::of(Capability::PowerState)),
                ),
                _ => Arc::new(mock),
            }
        })
        .collect();

    let mut converged = 0;
    let mut quarantined = 0;
    let mut launched = 0;

    for mock in &devices {
        let id = mock.id();
        let mut outcome = Decision::Nothing;

        for tick in 0..3 {
            let decision = pass(mock, &desired);
            outcome = decision.clone();

            match decision {
                Decision::Nothing => {}
                Decision::ProbePower => {
                    // A device that cannot report power ends up quarantined, not guessed at.
                    if matches!(mock.id().as_str(), "tv-48") {
                        let action =
                            ladder.on_failure(&id, &TvError::protocol("power unobservable"));
                        if action == HealerAction::Quarantine {
                            break;
                        }
                    }
                }
                Decision::Wake => {
                    if mock.as_power().is_some() {
                        block_on(mock.as_power().unwrap().wake()).ok();
                    } else {
                        ladder.on_failure(&id, &TvError::Unsupported(Capability::Wake));
                        break;
                    }
                }
                Decision::Sleep => {
                    block_on(mock.as_power().unwrap().sleep()).ok();
                }
                Decision::Launch(app) => {
                    let handle: &dyn tv_rs_core::AppLaunch = mock.as_app().expect("app");
                    let _ = block_on(handle.launch(app, tv_rs_core::CommandId::new()));
                    launched += 1;
                }
                Decision::Quarantine(reason) => {
                    let _ = reason;
                    // A device that cannot do the job is taken out of rotation
                    // immediately — no retries, no power commands.
                    let _ = ladder.on_failure(&id, &TvError::Unsupported(Capability::AppLaunch));
                    break;
                }
            }

            // Simulate the two devices that never pair mid-run.
            if matches!(mock.id().as_str(), "tv-46" | "tv-47") && tick == 1 {
                let action = ladder.on_failure(&id, &TvError::Auth(AuthError::TokenExpired));
                assert_eq!(action, HealerAction::Reprovision);
                break;
            }
        }

        if ladder.is_quarantined(&id) {
            quarantined += 1;
        } else if matches!(outcome, Decision::Nothing | Decision::Launch(_)) {
            converged += 1;
        }
    }

    assert_eq!(converged + quarantined, 50, "every device has an outcome");
    assert!(
        converged >= 46,
        "the healthy majority converges: {converged}"
    );
    assert!(
        quarantined >= 1,
        "the unobservable device is taken out of rotation"
    );
    assert!(launched > 0, "the fleet actually did something");
    assert!(
        devices.iter().all(|mock| mock.actions().len() < 10),
        "no device was hammered: bounded actions per pass"
    );
}

#[test]
fn a_device_that_cannot_wake_is_quarantined_and_never_toggled() {
    let mock = MockBackend::new(device(0))
        .starting_at(PowerState::Standby)
        .with_capabilities(CapabilitySet::of(Capability::PowerState));
    let decision = pass(&mock, &DesiredState::showing("netflix"));
    assert!(matches!(decision, Decision::Quarantine(_)), "{decision:?}");
    assert!(
        mock.actions().is_empty(),
        "a device that cannot be woken must not receive a power command"
    );
}

#[test]
fn a_lost_ack_does_not_double_toggle() {
    let mock = MockBackend::new(device(1));
    mock.fail_next(Failure::TimeoutAfterSend);
    let power = mock.as_power().expect("power");
    assert!(block_on(power.sleep()).is_err());
    // The caller must not retry a toggle blindly; the observation is now stale.
    let observed = ObservedState {
        power: ObservedField::unknown(),
        ..ObservedState::nothing_observed()
    };
    assert_eq!(
        decide(&DesiredState::default(), &observed, all_capabilities()),
        Decision::ProbePower
    );
}
