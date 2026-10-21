//! The graduated remediation ladder: what to *try* before a panel leaves rotation.
//!
//! The agent used to stop at a telemetry verdict — a hot panel was deferred, a
//! critical one was quarantined, and neither got a chance to recover. The ladder
//! adds the rungs between "notice" and "give up", in escalation order:
//!
//! | Rung | Symptom | Step | Capability |
//! |---|---|---|---|
//! | 1 | a panel that is hot but not critical | dim the backlight | [`Capability::Brightness`] |
//! | 2 | a live input whose player stopped | restart the foreground player | [`Capability::Key`] |
//! | 3 | still unhealthy, or critical | reboot the panel | [`Capability::Reboot`] |
//!
//! Like [`crate::decide`], every function here is pure: a snapshot, a verdict and the
//! device's capabilities go in, one step (or nothing) comes out. No clock, no counter,
//! no "third hot reading" state machine — the property that made the previous
//! behaviour easy to explain is kept, and the agent simply has something to do now.
//!
//! A rung whose capability is missing is **skipped, never faked**: a panel that
//! cannot dim is not "dimmed" by a log line, and a device that cannot be rebooted is
//! handed to a human instead of pretending the ladder ran. That is also why
//! [`plan`] returns `Option`: the caller records what it did, and an absent step is
//! an honest "nothing this ladder can do".
//!
//! The last rung is the only one the quarantine path takes. A panel taken out of
//! rotation is rebooted once — because a wedged panel often answers again after a
//! restart — and is reported as quarantined either way, so a reboot that does not
//! help never silently keeps a dead screen in the fleet.

use tv_rs_core::{Capability, CapabilitySet, Telemetry};

use crate::telemetry::{TelemetryPolicy, TelemetryVerdict};

/// The numbers the ladder needs that are not capabilities.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemedyPolicy {
    /// Backlight target for the dimming rung, 0-100.
    ///
    /// Low enough to shed heat, high enough that a viewer still sees a picture: a
    /// panel dimmed to zero is a panel that looks broken, which is a worse failure
    /// than a warm one.
    pub dim_to: u8,
}

impl Default for RemedyPolicy {
    fn default() -> Self {
        Self { dim_to: 20 }
    }
}

/// Why the ladder is being consulted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Symptom {
    /// Telemetry says the panel is hot but not critical.
    HotTemperature,
    /// Telemetry says the panel's temperature is critical.
    CriticalTemperature,
    /// Telemetry says the panel's link is unusable.
    UnusableLink,
    /// A live input whose foreground player stopped.
    ///
    /// This one is not a telemetry reading: no protocol in this workspace reports
    /// playback yet, so a caller that watches the player (or the LFD no-signal
    /// surface) names the symptom itself and gets the same ladder.
    PlaybackStalled,
}

/// One rung of the ladder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemedyStep {
    /// Dim the backlight to a percentage.
    DimBacklight {
        /// Target brightness, 0-100.
        to: u8,
    },
    /// Restart the foreground player.
    RestartPlayback,
    /// Reboot the panel.
    RebootPanel,
}

impl RemedyStep {
    /// A stable snake_case label for logs and audit entries.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DimBacklight { .. } => "dim_backlight",
            Self::RestartPlayback => "restart_playback",
            Self::RebootPanel => "reboot_panel",
        }
    }
}

/// The symptom a telemetry verdict describes, when it describes one the ladder acts on.
///
/// A `Wait` that is not about temperature is deliberately not a symptom: a weak link
/// is fixed by waiting for a better moment, not by dimming a panel or restarting it.
#[must_use]
pub fn symptom(
    telemetry: &Telemetry,
    verdict: TelemetryVerdict,
    policy: &TelemetryPolicy,
) -> Option<Symptom> {
    match verdict {
        TelemetryVerdict::Healthy => None,
        TelemetryVerdict::Wait(_) => {
            let temperature = telemetry.temperature_celsius.value?;
            (temperature >= policy.hot_temperature_celsius).then_some(Symptom::HotTemperature)
        }
        TelemetryVerdict::Quarantine(_) => {
            let temperature = telemetry.temperature_celsius.value;
            match temperature {
                Some(celsius) if celsius >= policy.critical_temperature_celsius => {
                    Some(Symptom::CriticalTemperature)
                }
                _ => Some(Symptom::UnusableLink),
            }
        }
    }
}

/// The step for a symptom, or `None` when the device cannot take it.
#[must_use]
pub fn plan(
    symptom: Symptom,
    capabilities: CapabilitySet,
    policy: &RemedyPolicy,
) -> Option<RemedyStep> {
    match symptom {
        Symptom::HotTemperature => capabilities
            .contains(Capability::Brightness)
            .then_some(RemedyStep::DimBacklight { to: policy.dim_to }),
        Symptom::PlaybackStalled => capabilities
            .contains(Capability::Key)
            .then_some(RemedyStep::RestartPlayback),
        Symptom::CriticalTemperature | Symptom::UnusableLink => capabilities
            .contains(Capability::Reboot)
            .then_some(RemedyStep::RebootPanel),
    }
}

/// The step for this snapshot and verdict, in one call.
#[must_use]
pub fn plan_remedy(
    telemetry: &Telemetry,
    verdict: TelemetryVerdict,
    capabilities: CapabilitySet,
    telemetry_policy: &TelemetryPolicy,
    remedy_policy: &RemedyPolicy,
) -> Option<RemedyStep> {
    symptom(telemetry, verdict, telemetry_policy)
        .and_then(|symptom| plan(symptom, capabilities, remedy_policy))
}

#[cfg(test)]
mod tests {
    use tv_rs_core::ObservedField;

    use super::*;

    fn hot(celsius: i16) -> Telemetry {
        Telemetry {
            temperature_celsius: ObservedField::from_read(celsius),
            ..Telemetry::nothing_observed()
        }
    }

    fn weak_link(strength: u8) -> Telemetry {
        Telemetry {
            signal_strength: ObservedField::from_read(strength),
            ..Telemetry::nothing_observed()
        }
    }

    fn capable(capabilities: &[Capability]) -> CapabilitySet {
        capabilities
            .iter()
            .fold(CapabilitySet::empty(), |set, capability| {
                set.with(*capability)
            })
    }

    #[test]
    fn a_hot_panel_dims_before_it_is_quarantined() {
        let telemetry = hot(70);
        let verdict = TelemetryVerdict::Wait("temperature is high");
        let step = plan_remedy(
            &telemetry,
            verdict,
            capable(&[Capability::Brightness, Capability::Reboot]),
            &TelemetryPolicy::default(),
            &RemedyPolicy::default(),
        );
        assert_eq!(step, Some(RemedyStep::DimBacklight { to: 20 }));
    }

    #[test]
    fn a_hot_panel_that_cannot_dim_gets_no_step_instead_of_a_fake_one() {
        let step = plan_remedy(
            &hot(70),
            TelemetryVerdict::Wait("temperature is high"),
            capable(&[Capability::PowerState, Capability::Key]),
            &TelemetryPolicy::default(),
            &RemedyPolicy::default(),
        );
        assert_eq!(step, None);
    }

    #[test]
    fn a_critical_panel_reboots_and_a_panel_that_cannot_reboot_does_not() {
        let telemetry = hot(85);
        let verdict = TelemetryVerdict::Quarantine("temperature is critical");
        assert_eq!(
            plan_remedy(
                &telemetry,
                verdict,
                capable(&[Capability::Reboot]),
                &TelemetryPolicy::default(),
                &RemedyPolicy::default(),
            ),
            Some(RemedyStep::RebootPanel)
        );
        assert_eq!(
            plan_remedy(
                &telemetry,
                verdict,
                capable(&[Capability::Brightness]),
                &TelemetryPolicy::default(),
                &RemedyPolicy::default(),
            ),
            None
        );
    }

    #[test]
    fn an_unusable_link_is_a_reboot_not_a_dim() {
        let telemetry = weak_link(5);
        let verdict = TelemetryVerdict::Quarantine("signal strength is unusable");
        assert_eq!(
            symptom(&telemetry, verdict, &TelemetryPolicy::default()),
            Some(Symptom::UnusableLink)
        );
        assert_eq!(
            plan_remedy(
                &telemetry,
                verdict,
                capable(&[Capability::Brightness, Capability::Reboot]),
                &TelemetryPolicy::default(),
                &RemedyPolicy::default(),
            ),
            Some(RemedyStep::RebootPanel)
        );
    }

    #[test]
    fn a_weak_link_waits_without_a_step() {
        let telemetry = weak_link(20);
        let verdict = TelemetryVerdict::Wait("signal strength is weak");
        assert_eq!(
            symptom(&telemetry, verdict, &TelemetryPolicy::default()),
            None
        );
        assert_eq!(
            plan_remedy(
                &telemetry,
                verdict,
                capable(&[Capability::Brightness, Capability::Reboot]),
                &TelemetryPolicy::default(),
                &RemedyPolicy::default(),
            ),
            None
        );
    }

    #[test]
    fn healthy_telemetry_has_no_symptom() {
        assert_eq!(
            symptom(
                &hot(42),
                TelemetryVerdict::Healthy,
                &TelemetryPolicy::default()
            ),
            None
        );
    }

    #[test]
    fn a_stalled_player_restarts_when_the_device_takes_keys() {
        assert_eq!(
            plan(
                Symptom::PlaybackStalled,
                capable(&[Capability::Key]),
                &RemedyPolicy::default()
            ),
            Some(RemedyStep::RestartPlayback)
        );
        assert_eq!(
            plan(
                Symptom::PlaybackStalled,
                capable(&[Capability::PowerState]),
                &RemedyPolicy::default()
            ),
            None
        );
    }

    #[test]
    fn the_dim_target_is_the_policy_number() {
        assert_eq!(
            plan(
                Symptom::HotTemperature,
                capable(&[Capability::Brightness]),
                &RemedyPolicy { dim_to: 35 }
            ),
            Some(RemedyStep::DimBacklight { to: 35 })
        );
    }

    #[test]
    fn steps_have_stable_labels() {
        assert_eq!(RemedyStep::DimBacklight { to: 1 }.as_str(), "dim_backlight");
        assert_eq!(RemedyStep::RestartPlayback.as_str(), "restart_playback");
        assert_eq!(RemedyStep::RebootPanel.as_str(), "reboot_panel");
    }
}
