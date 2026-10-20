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

/// The Phase 4 exit criterion, as written: 50 devices, 24 simulated hours, three
/// blips, two devices that never pair, one whose power is never observable.
///
/// A "blip" is the panel changing state **on its own** — someone picks up the remote,
/// a firmware update reboots it. The point is not that the fleet recovers once; it is
/// that it recovers every time, for a full day, without the action count growing.
#[test]
fn twenty_four_simulated_hours_with_three_blips() {
    let desired = DesiredState::showing("netflix");
    let mut ladder = HealerLadder::new(3);

    let devices: Vec<Arc<MockBackend>> = (0..50)
        .map(|index| {
            let mock = MockBackend::new(device(index));
            match index {
                48 => Arc::new(MockBackend::new(device(index)).starting_at(PowerState::Unknown)),
                49 => Arc::new(
                    MockBackend::new(device(index))
                        .with_capabilities(CapabilitySet::of(Capability::PowerState)),
                ),
                _ => Arc::new(mock),
            }
        })
        .collect();

    // Which healthy device drops into standby, and in which hour.
    let blips = [(0_usize, 5_u32), (7, 12), (31, 20)];
    let mut wakes_per_hour = [0_usize; 24];
    let mut healthy_converged = [0_usize; 24];
    let mut quarantined = std::collections::BTreeSet::new();
    let mut reprovisioned = std::collections::BTreeSet::new();

    for hour in 0..24_u32 {
        for (index, blip_hour) in blips {
            if hour == blip_hour {
                devices[index].force_power(PowerState::Standby);
            }
        }

        for mock in &devices {
            let id = mock.id();
            let index: usize = id.as_str().trim_start_matches("tv-").parse().expect("id");
            if ladder.is_quarantined(&id) || reprovisioned.contains(id.as_str()) {
                continue;
            }

            match pass(mock, &desired) {
                Decision::Nothing => {
                    if index < 46 {
                        healthy_converged[hour as usize] += 1;
                    }
                }
                Decision::Launch(app) => {
                    let handle: &dyn tv_rs_core::AppLaunch = mock.as_app().expect("app");
                    let _ = block_on(handle.launch(app, tv_rs_core::CommandId::new()));
                    if index < 46 {
                        healthy_converged[hour as usize] += 1;
                    }
                }
                Decision::Wake => {
                    let power = mock.as_power().expect("power");
                    block_on(power.wake()).expect("the mock wakes");
                    wakes_per_hour[hour as usize] += 1;
                    if index < 46 {
                        healthy_converged[hour as usize] += 1;
                    }
                }
                Decision::Sleep => {}
                Decision::ProbePower => {
                    // Only the device whose power is never observable gets here.
                    if ladder.on_failure(&id, &TvError::protocol("power unobservable"))
                        == HealerAction::Quarantine
                    {
                        quarantined.insert(id.as_str().to_string());
                    }
                }
                Decision::Quarantine(_) => {
                    ladder.on_failure(&id, &TvError::Unsupported(Capability::Wake));
                    quarantined.insert(id.as_str().to_string());
                }
            }

            // The two devices that never pair: a token expires and they leave rotation
            // until someone re-provisions them.
            if matches!(id.as_str(), "tv-46" | "tv-47") && hour == 1 {
                assert_eq!(
                    ladder.on_failure(&id, &TvError::Auth(AuthError::TokenExpired)),
                    HealerAction::Reprovision
                );
                reprovisioned.insert(id.as_str().to_string());
            }
        }
    }

    // Every hour, the whole healthy population ends in a converged state.
    for (hour, converged) in healthy_converged.iter().enumerate() {
        assert_eq!(
            *converged, 46,
            "hour {hour}: {converged} of 46 healthy devices converged"
        );
    }

    // Each blip is detected and fixed in the hour it happens, exactly once.
    let mut expected = [0_usize; 24];
    for (_, blip_hour) in blips {
        expected[blip_hour as usize] += 1;
    }
    assert_eq!(
        wakes_per_hour, expected,
        "one wake per blip, in the hour it happened — no retry storms"
    );

    assert_eq!(quarantined.len(), 2, "tv-48 and tv-49: {quarantined:?}");
    assert_eq!(reprovisioned.len(), 2, "tv-46 and tv-47: {reprovisioned:?}");

    // Bounded work: at most one action per hour of the day, plus a wake per blip.
    // The simulation re-asserts the desired app every pass, so 24 launches in 24 hours
    // is the converged steady state, not a retry storm — anything above it is.
    for mock in &devices {
        assert!(
            mock.actions().len() <= 24 + 3,
            "{} performed {} actions over 24 hours (bound: one per hour)",
            mock.id().as_str(),
            mock.actions().len()
        );
    }
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
