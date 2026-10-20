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
