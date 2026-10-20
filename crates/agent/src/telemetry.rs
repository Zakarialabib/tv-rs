//! Telemetry escalation: when a hot panel or a failing link should stop the loop.
//!
//! The decision is pure, like [`crate::decide`]. The only inputs are a snapshot and
//! thresholds; there is no clock, no network and no hidden counter. That matters
//! because the alternative ("quarantine after the third hot reading") turns a
//! hardware fault into a state machine that is hard to explain and impossible to
//! test without sleeping.
//!
//! Two levels are deliberate:
//!
//! - [`TelemetryVerdict::Wait`] means *do not touch this device this pass*. The
//!   capability still works; the panel needs to cool down or the radio needs a
//!   better moment.
//! - [`TelemetryVerdict::Quarantine`] means *take it out of rotation and tell a
//!   human*. Sending more commands cannot fix an overheating panel or a dead link.
//!
//! Unknown metrics are not healthy and not unhealthy: they are ignored, because a
//! protocol that does not expose a thermometer must not be quarantined for it.

use tv_rs_core::{Remedy, Telemetry};

/// Thresholds for turning telemetry into a decision.
///
/// The defaults are conservative values for commercial displays. A deployment with
/// panels in a hot enclosure, or a known-bad Wi-Fi site, should override them rather
/// than inherit a policy that does not describe its environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TelemetryPolicy {
    /// Temperature at or above which the agent stops touching the device.
    pub hot_temperature_celsius: i16,
    /// Temperature at or above which the device is taken out of rotation.
    pub critical_temperature_celsius: i16,
    /// Signal strength below which the agent waits.
    pub weak_signal_strength: u8,
    /// Signal strength at or below which the link is considered unusable.
    pub unusable_signal_strength: u8,
    /// Signal quality below which the agent waits.
    pub weak_signal_quality: u8,
    /// Signal quality at or below which the link is considered unusable.
    pub unusable_signal_quality: u8,
}

impl Default for TelemetryPolicy {
    fn default() -> Self {
        Self {
            hot_temperature_celsius: 65,
            critical_temperature_celsius: 80,
            weak_signal_strength: 35,
            unusable_signal_strength: 10,
            weak_signal_quality: 40,
            unusable_signal_quality: 15,
        }
    }
}

/// What telemetry says the agent should do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TelemetryVerdict {
    /// No metric crossed a threshold, or nothing was measured.
    Healthy,
    /// Stop for this pass; the device is still in rotation.
    Wait(&'static str),
    /// Stop until someone intervenes.
    Quarantine(&'static str),
}

impl TelemetryVerdict {
    /// The core remedy this verdict corresponds to.
    #[must_use]
    pub fn remedy(self) -> Option<Remedy> {
        match self {
            Self::Healthy => None,
            Self::Wait(_) => Some(Remedy::Wait),
            Self::Quarantine(_) => Some(Remedy::Quarantine),
        }
    }

    /// The human-readable reason, when the verdict is not healthy.
    #[must_use]
    pub fn reason(self) -> Option<&'static str> {
        match self {
            Self::Healthy => None,
            Self::Wait(reason) | Self::Quarantine(reason) => Some(reason),
        }
    }

    /// Whether the verdict permits normal reconciliation.
    #[must_use]
    pub fn is_healthy(self) -> bool {
        matches!(self, Self::Healthy)
    }
}

/// Assesses one telemetry snapshot.
///
/// Temperature is checked before signal metrics: a panel that is both hot and
/// weakly connected should be reported as hot, because that is the condition more
/// likely to cause permanent damage. Within each metric, the critical threshold is
/// checked before the warning threshold.
#[must_use]
pub fn assess_telemetry(telemetry: &Telemetry, policy: &TelemetryPolicy) -> TelemetryVerdict {
    if let Some(temperature) = telemetry.temperature_celsius.value {
        if temperature >= policy.critical_temperature_celsius {
            return TelemetryVerdict::Quarantine("temperature is critical");
        }
        if temperature >= policy.hot_temperature_celsius {
            return TelemetryVerdict::Wait("temperature is high");
        }
    }

    if let Some(strength) = telemetry.signal_strength.value {
        if strength <= policy.unusable_signal_strength {
            return TelemetryVerdict::Quarantine("signal strength is unusable");
        }
        if strength < policy.weak_signal_strength {
            return TelemetryVerdict::Wait("signal strength is weak");
        }
    }

    if let Some(quality) = telemetry.signal_quality.value {
        if quality <= policy.unusable_signal_quality {
            return TelemetryVerdict::Quarantine("signal quality is unusable");
        }
        if quality < policy.weak_signal_quality {
            return TelemetryVerdict::Wait("signal quality is weak");
        }
    }

    TelemetryVerdict::Healthy
}

#[cfg(test)]
mod tests {
    use tv_rs_core::ObservedField;

    use super::*;

    fn with_temperature(celsius: i16) -> Telemetry {
        Telemetry {
            temperature_celsius: ObservedField::from_read(celsius),
            ..Telemetry::nothing_observed()
        }
    }

    fn with_strength(percent: u8) -> Telemetry {
        Telemetry {
            signal_strength: ObservedField::from_read(percent),
            ..Telemetry::nothing_observed()
        }
    }

    fn with_quality(percent: u8) -> Telemetry {
        Telemetry {
            signal_quality: ObservedField::from_read(percent),
            ..Telemetry::nothing_observed()
        }
    }

    #[test]
    fn missing_metrics_are_not_a_fault() {
        assert_eq!(
            assess_telemetry(&Telemetry::nothing_observed(), &TelemetryPolicy::default()),
            TelemetryVerdict::Healthy
        );
    }

    #[test]
    fn a_hot_panel_waits_and_a_critical_panel_is_quarantined() {
        let policy = TelemetryPolicy::default();
        assert_eq!(
            assess_telemetry(&with_temperature(65), &policy),
            TelemetryVerdict::Wait("temperature is high")
        );
        assert_eq!(
            assess_telemetry(&with_temperature(80), &policy),
            TelemetryVerdict::Quarantine("temperature is critical")
        );
    }

    #[test]
    fn a_weak_link_waits_and_a_dead_link_is_quarantined() {
        let policy = TelemetryPolicy::default();
        assert_eq!(
            assess_telemetry(&with_strength(34), &policy),
            TelemetryVerdict::Wait("signal strength is weak")
        );
        assert_eq!(
            assess_telemetry(&with_strength(10), &policy),
            TelemetryVerdict::Quarantine("signal strength is unusable")
        );
        assert_eq!(
            assess_telemetry(&with_quality(39), &policy),
            TelemetryVerdict::Wait("signal quality is weak")
        );
        assert_eq!(
            assess_telemetry(&with_quality(15), &policy),
            TelemetryVerdict::Quarantine("signal quality is unusable")
        );
    }

    #[test]
    fn healthy_metrics_permit_reconciliation() {
        let telemetry = Telemetry {
            temperature_celsius: ObservedField::from_read(42),
            signal_strength: ObservedField::from_read(80),
            signal_quality: ObservedField::from_read(90),
            ..Telemetry::nothing_observed()
        };
        assert!(assess_telemetry(&telemetry, &TelemetryPolicy::default()).is_healthy());
    }

    #[test]
    fn a_critical_temperature_wins_over_a_weak_signal() {
        let telemetry = Telemetry {
            temperature_celsius: ObservedField::from_read(90),
            signal_strength: ObservedField::from_read(20),
            ..Telemetry::nothing_observed()
        };
        assert_eq!(
            assess_telemetry(&telemetry, &TelemetryPolicy::default()),
            TelemetryVerdict::Quarantine("temperature is critical")
        );
    }

    #[test]
    fn verdicts_map_to_core_remedies() {
        assert_eq!(TelemetryVerdict::Healthy.remedy(), None);
        assert_eq!(TelemetryVerdict::Wait("warm").remedy(), Some(Remedy::Wait));
        assert_eq!(
            TelemetryVerdict::Quarantine("hot").remedy(),
            Some(Remedy::Quarantine)
        );
    }
}
