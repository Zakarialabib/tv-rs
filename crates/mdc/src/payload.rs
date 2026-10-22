//! Typed views of MDC query replies.

use crate::error::MdcError;
use crate::types::{InputSource, MdcPower};

/// The result of the status command (`0x00`).
///
/// Volume and mute are optional because panels without audio return `0xFF`
/// for both. The aspect and timer fields are preserved as raw bytes: they are
/// model-specific and changing them is outside the control surface this crate
/// exposes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    /// Power state reported by the display.
    pub power: MdcPower,
    /// Volume 0-100, or `None` when the panel has no audio.
    pub volume: Option<u8>,
    /// Mute state, or `None` when the panel has no audio.
    pub muted: Option<bool>,
    /// Active input, or `None` when no source is selected.
    pub input: Option<InputSource>,
    /// Picture aspect code.
    pub aspect: u8,
    /// Old on-timer field, retained for compatibility with the status layout.
    pub on_timer_nf: u8,
    /// Old off-timer field, retained for compatibility with the status layout.
    pub off_timer_nf: u8,
}

impl Status {
    /// Parses the payload of an acknowledgement to the status command.
    ///
    /// # Errors
    ///
    /// Returns [`MdcError::Protocol`] when the payload is truncated or a known
    /// field carries a value outside its documented range.
    pub fn from_payload(data: &[u8]) -> Result<Self, MdcError> {
        if data.len() < 5 {
            return Err(MdcError::Protocol(format!(
                "status reply carries {} bytes; at least 5 are required",
                data.len()
            )));
        }

        let power = MdcPower::from_code(data[0])
            .ok_or_else(|| MdcError::Protocol(format!("unknown power code 0x{:02X}", data[0])))?;
        let volume = match data[1] {
            0xFF => None,
            value if value <= 100 => Some(value),
            value => {
                return Err(MdcError::Protocol(format!(
                    "volume 0x{value:02X} is outside 0-100"
                )))
            }
        };
        let muted = match data[2] {
            0x00 => Some(false),
            0x01 => Some(true),
            0xFF => None,
            value => {
                return Err(MdcError::Protocol(format!(
                    "mute code 0x{value:02X} is not 0, 1 or 0xFF"
                )))
            }
        };

        Ok(Self {
            power,
            volume,
            muted,
            input: InputSource::from_code(data[3]),
            aspect: data[4],
            on_timer_nf: data.get(5).copied().unwrap_or(0),
            off_timer_nf: data.get(6).copied().unwrap_or(0),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Status;
    use crate::types::{InputSource, MdcPower};

    #[test]
    fn status_payload_is_typed() {
        let status = Status::from_payload(&[1, 50, 0, 0x21, 1, 0, 0]).unwrap();
        assert_eq!(status.power, MdcPower::On);
        assert_eq!(status.volume, Some(50));
        assert_eq!(status.muted, Some(false));
        assert_eq!(status.input, Some(InputSource::Hdmi1));
        assert_eq!(status.aspect, 1);
    }

    #[test]
    fn an_audio_less_panel_reports_unknown_volume_and_mute() {
        let status = Status::from_payload(&[1, 0xFF, 0xFF, 0x40, 0]).unwrap();
        assert_eq!(status.volume, None);
        assert_eq!(status.muted, None);
        assert_eq!(status.input, Some(InputSource::Tv));
        assert_eq!(status.on_timer_nf, 0);
    }

    #[test]
    fn truncated_and_out_of_range_payloads_fail() {
        assert!(Status::from_payload(&[1, 50]).is_err());
        assert!(Status::from_payload(&[1, 101, 0, 0x21, 1]).is_err());
        assert!(Status::from_payload(&[1, 50, 2, 0x21, 1]).is_err());
    }
}

/// The result of the error-status command (`0x0D`).
///
/// This is the panel's own health report, and it is the one MDC command that answers
/// the two questions a playout monitor asks: *is a signal arriving?* (the input-source
/// error flag) and *is the panel well?* (lamp, temperature, fan). A panel whose input
/// has no sync is not a broken panel — the source is — and the two must not be
/// collapsed into one "fault" bit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErrorStatus {
    /// The panel reports a lamp (backlight) fault.
    pub lamp_error: bool,
    /// The panel reports a temperature fault.
    pub temperature_error: bool,
    /// The active input has no sync, or a resolution the panel cannot lock onto.
    pub signal_error: bool,
    /// Panel temperature in Celsius, as the panel reports it.
    pub temperature_celsius: u8,
    /// The panel reports a fan fault.
    pub fan_error: bool,
}

impl ErrorStatus {
    /// Parses the payload of an acknowledgement to the error-status command.
    ///
    /// # Errors
    ///
    /// Returns [`MdcError::Protocol`] when the payload is truncated or a flag carries
    /// a value the specification does not define.
    pub fn from_payload(data: &[u8]) -> Result<Self, MdcError> {
        if data.len() < 5 {
            return Err(MdcError::Protocol(format!(
                "error status reply carries {} bytes; at least 5 are required",
                data.len()
            )));
        }

        let flag = |byte: u8, what: &str| -> Result<bool, MdcError> {
            match byte {
                0x00 => Ok(false),
                0x01 => Ok(true),
                other => Err(MdcError::Protocol(format!(
                    "{what} code 0x{other:02X} is neither 0 (normal) nor 1 (error)"
                ))),
            }
        };

        // The input-source field also reports 0x02 for an invalid selection, which is
        // a signal problem for this purpose: nothing is being displayed.
        let signal_error = match data[3] {
            0x00 => false,
            0x01 | 0x02 => true,
            other => {
                return Err(MdcError::Protocol(format!(
                    "input-source error code 0x{other:02X} is outside 0-2"
                )))
            }
        };

        Ok(Self {
            lamp_error: flag(data[0], "lamp error")?,
            temperature_error: flag(data[1], "temperature error")?,
            signal_error,
            temperature_celsius: data[4],
            fan_error: flag(data.get(5).copied().unwrap_or(0x00), "fan error")?,
        })
    }
}

/// The result of the panel-on-time command (`0x83`).
///
/// The wire value is in units of ten minutes; the conversion to hours is done here so
/// no caller has to remember it.
#[must_use]
pub fn panel_hours_from_payload(data: &[u8]) -> Option<u32> {
    if data.is_empty() {
        return None;
    }
    let ten_minutes: u32 = data
        .iter()
        .fold(0_u32, |value, byte| (value << 8) | u32::from(*byte));
    Some(ten_minutes / 6)
}
