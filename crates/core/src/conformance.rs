//! A backend conformance suite that runs against anything implementing [`DeviceHandle`].
//!
//! Every backend must satisfy these checks before it is considered done — including
//! the mock, which is how the checks themselves are validated. The suite is
//! deliberately narrow: it asserts the properties that cannot be verified by a
//! successful call, which are exactly the ones that rot.
//!
//! ```no_run
//! use tv_rs_core::conformance;
//!
//! # async fn example(handle: &dyn tv_rs_core::DeviceHandle) {
//! let violations = conformance::run(handle).await;
//! assert!(violations.is_empty(), "{violations:#?}");
//! # }
//! ```
//!
//! # What it does not check
//!
//! Confidence discipline (`High` vs `Inferred`) is a property of the values a
//! backend *reports*, and [`DeviceHandle`] exposes no observed state. That check
//! belongs to the agent, once a backend can be asked for its [`ObservedState`].
//!
//! [`ObservedState`]: crate::ObservedState

use crate::capability::{AppRef, Capability, DeviceHandle, Input, Key};
use crate::command::CommandId;
use crate::error::TvError;

/// One failed expectation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    /// The check that failed, named as it appears in this module.
    pub check: &'static str,
    /// What was expected and what happened.
    pub detail: String,
}

impl Violation {
    fn new(check: &'static str, detail: impl Into<String>) -> Self {
        Self {
            check,
            detail: detail.into(),
        }
    }
}

/// Runs every check and returns the violations found, in check order.
///
/// An empty vector means the handle conforms. Errors are *not* returned: a
/// conformance failure is data, not an error condition for the caller.
pub async fn run(handle: &dyn DeviceHandle) -> Vec<Violation> {
    let mut violations = Vec::new();
    capability_accessors(handle, &mut violations);
    unsupported_routing(handle, &mut violations).await;
    identity(handle, &mut violations);
    violations
}

/// Every advertised capability must have an accessor, and every unadvertised one
/// must not.
fn capability_accessors(handle: &dyn DeviceHandle, violations: &mut Vec<Violation>) {
    let set = handle.capabilities();

    let pairs = [
        (Capability::PowerState, handle.as_power().is_some()),
        (Capability::Key, handle.as_key().is_some()),
        (Capability::AppLaunch, handle.as_app().is_some()),
        (Capability::InputSelect, handle.as_input().is_some()),
        (Capability::Volume, handle.as_volume().is_some()),
        (Capability::Brightness, handle.as_brightness().is_some()),
        (Capability::Reboot, handle.as_reboot().is_some()),
        (Capability::Telemetry, handle.as_telemetry().is_some()),
    ];

    for (capability, accessor) in pairs {
        let advertised = set.contains(capability);
        if advertised != accessor {
            violations.push(Violation::new(
                "capability_accessors",
                format!("{capability}: advertised={advertised} but accessor_present={accessor}"),
            ));
        }
    }
}

/// A capability the handle does not advertise must fail with `Unsupported`, so a
/// caller can branch on the remedy instead of parsing a message.
async fn unsupported_routing(handle: &dyn DeviceHandle, violations: &mut Vec<Violation>) {
    let set = handle.capabilities();

    if !set.contains(Capability::Key) {
        if let Some(key) = handle.as_key() {
            match key.key_press(Key::Home, CommandId::new()).await {
                Err(TvError::Unsupported(Capability::Key)) => {}
                other => violations.push(Violation::new(
                    "unsupported_routing",
                    format!("key_press without Key capability returned {other:?}"),
                )),
            }
        }
    }

    if !set.contains(Capability::InputSelect) {
        if let Some(input) = handle.as_input() {
            match input.select_input(Input::Hdmi(1), CommandId::new()).await {
                Err(TvError::Unsupported(Capability::InputSelect)) => {}
                other => violations.push(Violation::new(
                    "unsupported_routing",
                    format!("select_input without InputSelect returned {other:?}"),
                )),
            }
        }
    }

    if !set.contains(Capability::Wake) {
        if let Some(power) = handle.as_power() {
            match power.wake().await {
                Err(TvError::Unsupported(Capability::Wake)) => {}
                other => violations.push(Violation::new(
                    "unsupported_routing",
                    format!("wake without Wake capability returned {other:?}"),
                )),
            }
        }
    }

    if !set.contains(Capability::Brightness) {
        if let Some(brightness) = handle.as_brightness() {
            match brightness.set_brightness(50, CommandId::new()).await {
                Err(TvError::Unsupported(Capability::Brightness)) => {}
                other => violations.push(Violation::new(
                    "unsupported_routing",
                    format!("set_brightness without Brightness returned {other:?}"),
                )),
            }
        }
    }

    if !set.contains(Capability::Reboot) {
        if let Some(reboot) = handle.as_reboot() {
            match reboot.reboot(CommandId::new()).await {
                Err(TvError::Unsupported(Capability::Reboot)) => {}
                other => violations.push(Violation::new(
                    "unsupported_routing",
                    format!("reboot without Reboot returned {other:?}"),
                )),
            }
        }
    }

    if !set.contains(Capability::Telemetry) {
        if let Some(telemetry) = handle.as_telemetry() {
            match telemetry.telemetry().await {
                Err(TvError::Unsupported(Capability::Telemetry)) => {}
                other => violations.push(Violation::new(
                    "unsupported_routing",
                    format!("telemetry without Telemetry returned {other:?}"),
                )),
            }
        }
    }

    // `foreground` is part of AppLaunch but not every platform can answer it; an
    // `Unsupported` there is acceptable, a panic or a transport error is not.
    if let Some(app) = handle.as_app() {
        if !set.contains(Capability::Foreground) {
            match app.foreground().await {
                Err(TvError::Unsupported(Capability::Foreground)) | Ok(None) => {}
                other => violations.push(Violation::new(
                    "unsupported_routing",
                    format!("foreground without Foreground capability returned {other:?}"),
                )),
            }
        }
    }

    let _ = AppRef::new("conformance-probe");
}

/// Identity must be usable as a map key: non-empty, and stable across calls.
fn identity(handle: &dyn DeviceHandle, violations: &mut Vec<Violation>) {
    let first = handle.id();
    if first.as_str().trim().is_empty() {
        violations.push(Violation::new("identity", "id() is empty"));
    }
    if first != handle.id() {
        violations.push(Violation::new(
            "identity",
            "id() is not stable across calls",
        ));
    }
    if handle.descriptor().address.trim().is_empty() {
        violations.push(Violation::new("identity", "descriptor().address is empty"));
    }
}

#[cfg(all(test, feature = "mock-backend"))]
mod tests {
    use futures::executor::block_on;

    use super::run;
    use crate::capability::Capability;
    use crate::capability::CapabilitySet;
    use crate::descriptor::{DeviceDescriptor, DeviceId, DeviceOs};
    use crate::mock::MockBackend;

    fn descriptor() -> DeviceDescriptor {
        DeviceDescriptor::new(
            DeviceId::new("conformance-1").unwrap(),
            DeviceOs::Roku,
            "10.0.0.7",
        )
    }

    #[test]
    fn a_full_mock_conforms() {
        let mock = MockBackend::new(descriptor());
        let violations = block_on(run(&mock));
        assert!(violations.is_empty(), "{violations:#?}");
    }

    #[test]
    fn a_minimal_mock_conforms() {
        let mock = MockBackend::new(descriptor())
            .with_capabilities(CapabilitySet::of(Capability::PowerState));
        let violations = block_on(run(&mock));
        assert!(violations.is_empty(), "{violations:#?}");
    }
}
