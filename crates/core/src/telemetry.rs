//! Diagnostic telemetry: what a panel reports about its own health.
//!
//! Telemetry is deliberately separate from [`ObservedState`](crate::ObservedState).
//! Power, input and volume describe what the screen is *doing*; temperature,
//! signal and uptime describe whether it can keep doing it. A fleet needs the
//! second set to distinguish "the app crashed" from "the panel is overheating" or
//! "the Wi-Fi link is falling apart".
//!
//! Every field is an [`ObservedField`], because a panel that
//! stops answering is not cool and well-fed: the value is unknown.

use std::time::Duration;

use async_trait::async_trait;

use crate::error::TvError;
use crate::state::ObservedField;

/// A snapshot of a device's diagnostic metrics.
///
/// A backend fills only what its protocol exposes; everything else stays
/// [`ObservedField::unknown`]. Missing telemetry is not an error, and a caller must
/// not treat an absent metric as a healthy one.
#[derive(Debug, Clone, Default)]
pub struct Telemetry {
    /// Panel temperature in whole degrees Celsius.
    pub temperature_celsius: ObservedField<i16>,
    /// Radio signal strength as a percentage, 0-100.
    pub signal_strength: ObservedField<u8>,
    /// Radio signal quality as a percentage, 0-100.
    pub signal_quality: ObservedField<u8>,
    /// Time since the device last booted, when the protocol reports it.
    pub uptime: ObservedField<Duration>,
}

impl Telemetry {
    /// A snapshot in which nothing is known.
    #[must_use]
    pub fn nothing_observed() -> Self {
        Self::default()
    }

    /// Whether every metric is unknown.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        !self.temperature_celsius.is_known()
            && !self.signal_strength.is_known()
            && !self.signal_quality.is_known()
            && !self.uptime.is_known()
    }
}

/// A backend that can report [`Telemetry`].
///
/// The method is fallible because the read still uses the control channel: a
/// transport failure is not the same as "this panel has no thermometer".
#[async_trait]
pub trait TelemetryProvider: Send + Sync {
    /// Reads one telemetry snapshot.
    async fn telemetry(&self) -> Result<Telemetry, TvError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_observed_is_empty() {
        assert!(Telemetry::nothing_observed().is_empty());
    }

    #[test]
    fn one_known_metric_is_not_empty() {
        let telemetry = Telemetry {
            temperature_celsius: ObservedField::from_read(42),
            ..Telemetry::nothing_observed()
        };
        assert!(!telemetry.is_empty());
        assert_eq!(telemetry.temperature_celsius.value, Some(42));
    }
}
