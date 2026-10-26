//! Payload structures for Philips SICP status responses.

use crate::error::SicpError;
use crate::types::{SicpInput, SicpPower};

/// Parsed display status and health telemetry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    /// Power state.
    pub power: SicpPower,
    /// Current volume level (0–100).
    pub volume: u8,
    /// Active input source.
    pub input: SicpInput,
    /// Internal panel temperature in Celsius.
    pub temperature_celsius: i8,
    /// True if input signal is present.
    pub signal_present: bool,
    /// True if lamp / backlight error detected.
    pub lamp_error: bool,
}

impl Status {
    /// Parses status telemetry payload bytes returned by temperature/status query (`0x2F`).
    ///
    /// Expected format: `[temperature_celsius (i8), signal_flag (0/1), lamp_error_flag (0/1)]`
    ///
    /// # Errors
    ///
    /// Returns [`SicpError::Protocol`] if payload is truncated.
    pub fn from_telemetry_payload(
        power: SicpPower,
        volume: u8,
        input: SicpInput,
        payload: &[u8],
    ) -> Result<Self, SicpError> {
        if payload.len() < 3 {
            return Err(SicpError::Protocol(format!(
                "telemetry payload requires at least 3 bytes; got {}",
                payload.len()
            )));
        }

        let temperature_celsius = payload[0] as i8;
        let signal_present = payload[1] != 0;
        let lamp_error = payload[2] != 0;

        Ok(Self {
            power,
            volume,
            input,
            temperature_celsius,
            signal_present,
            lamp_error,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_telemetry_payload() {
        let payload = [38i8 as u8, 1, 0];
        let status =
            Status::from_telemetry_payload(SicpPower::On, 45, SicpInput::Hdmi1, &payload).unwrap();
        assert_eq!(status.power, SicpPower::On);
        assert_eq!(status.volume, 45);
        assert_eq!(status.input, SicpInput::Hdmi1);
        assert_eq!(status.temperature_celsius, 38);
        assert!(status.signal_present);
        assert!(!status.lamp_error);
    }
}
