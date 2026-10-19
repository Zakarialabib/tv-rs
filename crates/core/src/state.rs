//! Observed state and the confidence attached to it.

use std::time::{Duration, Instant};

use crate::capability::{AppRef, Input};
use crate::descriptor::PowerState;

/// How much a stored value can be trusted.
///
/// The ordering is meaningful: `Unknown < Inferred < High`, so a caller can write
/// `if field.confidence >= Confidence::Inferred`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Confidence {
    /// Nothing is known, or what was known has expired.
    #[default]
    Unknown,
    /// The device accepted a request but did not confirm the outcome, or the value
    /// was inferred from a previous state.
    Inferred,
    /// The device reported this value.
    High,
}

impl Confidence {
    /// A stable label for logs and UI.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Inferred => "inferred",
            Self::High => "high",
        }
    }
}

/// A value plus when and how confidently it was established.
#[derive(Debug, Clone)]
pub struct ObservedField<T> {
    /// The value, or `None` when nothing is known.
    pub value: Option<T>,
    /// How sure we are.
    pub confidence: Confidence,
    /// When the value was established (monotonic clock).
    pub last_verified_at: Instant,
}

impl<T> ObservedField<T> {
    /// Nothing known.
    #[must_use]
    pub fn unknown() -> Self {
        Self {
            value: None,
            confidence: Confidence::Unknown,
            last_verified_at: Instant::now(),
        }
    }

    /// A value the device reported: the only constructor that yields `High`.
    #[must_use]
    pub fn from_read(value: T) -> Self {
        Self {
            value: Some(value),
            confidence: Confidence::High,
            last_verified_at: Instant::now(),
        }
    }

    /// A value a write confirmed with a state change.
    ///
    /// Distinct from [`ObservedField::from_read`] in name only — the important part
    /// is that a backend must not use it for a write it could not verify.
    #[must_use]
    pub fn from_acked_write(value: T) -> Self {
        Self::from_read(value)
    }

    /// A value a write implied but did not prove.
    ///
    /// Use for `POST /launch` returning 200, for `set_volume` on a platform that
    /// only accepts relative keys, and for anything the vendor might refuse
    /// silently while reporting success.
    #[must_use]
    pub fn from_unverified_write(value: T) -> Self {
        Self {
            value: Some(value),
            confidence: Confidence::Inferred,
            last_verified_at: Instant::now(),
        }
    }

    /// A value produced by a toggle.
    ///
    /// A toggle is never `High`: `KEY_POWER` does not mean "now off", it means "the
    /// request to change power was accepted".
    #[must_use]
    pub fn from_toggle(value: T) -> Self {
        Self::from_unverified_write(value)
    }

    /// Forgets the value after a failure.
    ///
    /// A failed write must not leave a stale `High` claim behind: that is how a
    /// fleet reports "on" about a TV that is unplugged.
    pub fn invalidate(&mut self) {
        self.value = None;
        self.confidence = Confidence::Unknown;
        self.last_verified_at = Instant::now();
    }

    /// Ages the value: `High` becomes `Inferred` after `high_to_inferred`, and
    /// anything becomes `Unknown` after `inferred_to_unknown`.
    ///
    /// The durations are arguments rather than constants because the right window is
    /// per-deployment and per-OS; the agent crate owns the defaults.
    pub fn decay(&mut self, high_to_inferred: Duration, inferred_to_unknown: Duration) {
        let age = self.last_verified_at.elapsed();
        match self.confidence {
            Confidence::High if age >= high_to_inferred => {
                self.confidence = Confidence::Inferred;
            }
            Confidence::Inferred if age >= inferred_to_unknown => {
                self.value = None;
                self.confidence = Confidence::Unknown;
            }
            _ => {}
        }
    }

    /// Whether the value is device-reported.
    #[must_use]
    pub fn is_high(&self) -> bool {
        self.confidence == Confidence::High
    }

    /// Whether any value is known at all.
    #[must_use]
    pub fn is_known(&self) -> bool {
        self.value.is_some()
    }
}

impl<T> Default for ObservedField<T> {
    fn default() -> Self {
        Self::unknown()
    }
}

/// The state a fleet keeps per device.
#[derive(Debug, Clone, Default)]
pub struct ObservedState {
    /// Power.
    pub power: ObservedField<PowerState>,
    /// Active input.
    pub input: ObservedField<Input>,
    /// Foreground application.
    pub foreground: ObservedField<AppRef>,
    /// Volume, 0–100.
    pub volume: ObservedField<u8>,
}

impl ObservedState {
    /// A state in which nothing is known — the only honest starting point.
    #[must_use]
    pub fn nothing_observed() -> Self {
        Self::default()
    }

    /// Whether every field is `Unknown`.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        !self.power.is_known()
            && !self.input.is_known()
            && !self.foreground.is_known()
            && !self.volume.is_known()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{Confidence, ObservedField, ObservedState};

    #[test]
    fn a_read_is_high_and_a_toggle_is_not() {
        assert_eq!(ObservedField::from_read(1u8).confidence, Confidence::High);
        assert_eq!(
            ObservedField::from_unverified_write(1u8).confidence,
            Confidence::Inferred
        );
        assert_eq!(
            ObservedField::from_toggle(1u8).confidence,
            Confidence::Inferred
        );
    }

    #[test]
    fn a_failed_write_forgets_the_previous_value() {
        let mut field = ObservedField::from_read(true);
        field.invalidate();
        assert_eq!(field.confidence, Confidence::Unknown);
        assert!(field.value.is_none());
    }

    #[test]
    fn decay_is_a_policy_the_caller_supplies() {
        let mut field = ObservedField::from_read("on");
        field.decay(Duration::from_secs(120), Duration::from_secs(300));
        assert_eq!(field.confidence, Confidence::High, "fresh values stay high");

        // Zero windows age everything immediately, which is how the agent tests
        // drive the transition without sleeping.
        field.decay(Duration::ZERO, Duration::from_secs(300));
        assert_eq!(field.confidence, Confidence::Inferred);
        assert_eq!(field.value, Some("on"), "inference keeps the value");

        field.decay(Duration::ZERO, Duration::ZERO);
        assert_eq!(field.confidence, Confidence::Unknown);
        assert!(field.value.is_none(), "expired values are dropped");
    }

    #[test]
    fn nothing_observed_is_the_default_state() {
        let state = ObservedState::nothing_observed();
        assert!(state.is_empty());
        assert_eq!(state.power.confidence, Confidence::Unknown);
    }
}
