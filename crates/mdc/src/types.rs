//! Display ids, power states and MDC input-source codes.

use std::fmt;

use crate::error::MdcError;

/// The address of one display, or of every display on the wire.
///
/// MDC uses `0x00`-`0xFD` for individual panels and `0xFE` for a broadcast.
/// `0xFF` is reserved for the reply header and is rejected here so a malformed
/// address can never be confused with a response.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DisplayId(u8);

impl DisplayId {
    /// The broadcast address. Broadcast commands do not receive an acknowledgement.
    pub const BROADCAST: Self = Self(0xFE);

    /// The wired/direct display address.
    pub const DIRECT: Self = Self(0x00);

    /// Wraps an address in `0x00..=0xFE`.
    ///
    /// # Errors
    ///
    /// Returns [`MdcError::InvalidDisplayId`] for `0xFF`.
    pub fn new(value: u8) -> Result<Self, MdcError> {
        if value == 0xFF {
            return Err(MdcError::InvalidDisplayId(value));
        }
        Ok(Self(value))
    }

    /// The wire value.
    #[must_use]
    pub fn value(self) -> u8 {
        self.0
    }

    /// Whether this address targets every display and therefore has no reply.
    #[must_use]
    pub fn is_broadcast(self) -> bool {
        self == Self::BROADCAST
    }
}

impl Default for DisplayId {
    fn default() -> Self {
        Self::DIRECT
    }
}

impl fmt::Display for DisplayId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_broadcast() {
            formatter.write_str("broadcast")
        } else {
            write!(formatter, "{}", self.0)
        }
    }
}

/// The value carried by the power command (`0x11`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MdcPower {
    /// Standby/off.
    Off,
    /// On.
    On,
    /// Reboot the display.
    Reboot,
}

impl MdcPower {
    /// The wire value.
    #[must_use]
    pub fn code(self) -> u8 {
        match self {
            Self::Off => 0x00,
            Self::On => 0x01,
            Self::Reboot => 0x02,
        }
    }

    /// Parses a power byte from a status or acknowledgement.
    #[must_use]
    pub fn from_code(code: u8) -> Option<Self> {
        match code {
            0x00 => Some(Self::Off),
            0x01 => Some(Self::On),
            0x02 => Some(Self::Reboot),
            _ => None,
        }
    }
}

/// An MDC input-source code.
///
/// The enum covers the settable sources used by signage as named variants.
/// Read-only variants (for example `HDMI1_PC`) and vendor-specific codes are
/// preserved as [`InputSource::Other`] so a status read never loses information.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InputSource {
    /// HDMI 1 (`0x21`).
    Hdmi1,
    /// HDMI 2 (`0x23`).
    Hdmi2,
    /// HDMI 3 (`0x31`).
    Hdmi3,
    /// HDMI 4 (`0x33`).
    Hdmi4,
    /// DisplayPort 1 (`0x25`).
    DisplayPort1,
    /// DisplayPort 2 (`0x26`).
    DisplayPort2,
    /// DisplayPort 3 (`0x27`).
    DisplayPort3,
    /// Digital TV (`0x40`).
    Tv,
    /// Internal/USB media source (`0x62`).
    Usb,
    /// Composite AV (`0x0C`).
    Av,
    /// Component (`0x08`).
    Component,
    /// Any other MDC source code.
    Other(u8),
}

impl InputSource {
    /// The wire value.
    #[must_use]
    pub fn code(self) -> u8 {
        match self {
            Self::Hdmi1 => 0x21,
            Self::Hdmi2 => 0x23,
            Self::Hdmi3 => 0x31,
            Self::Hdmi4 => 0x33,
            Self::DisplayPort1 => 0x25,
            Self::DisplayPort2 => 0x26,
            Self::DisplayPort3 => 0x27,
            Self::Tv => 0x40,
            Self::Usb => 0x62,
            Self::Av => 0x0C,
            Self::Component => 0x08,
            Self::Other(code) => code,
        }
    }

    /// Parses a source byte. `0x00` means "no source selected" and returns `None`.
    #[must_use]
    pub fn from_code(code: u8) -> Option<Self> {
        match code {
            0x00 => None,
            0x21 => Some(Self::Hdmi1),
            0x23 => Some(Self::Hdmi2),
            0x31 => Some(Self::Hdmi3),
            0x33 => Some(Self::Hdmi4),
            0x25 => Some(Self::DisplayPort1),
            0x26 => Some(Self::DisplayPort2),
            0x27 => Some(Self::DisplayPort3),
            0x40 => Some(Self::Tv),
            0x62 => Some(Self::Usb),
            0x0C => Some(Self::Av),
            0x08 => Some(Self::Component),
            other => Some(Self::Other(other)),
        }
    }

    /// Maps a vendor-neutral input to an MDC source, when one exists.
    #[must_use]
    pub fn from_core_input(input: &tv_rs_core::Input) -> Option<Self> {
        use tv_rs_core::Input;

        match input {
            Input::Hdmi(1) => Some(Self::Hdmi1),
            Input::Hdmi(2) => Some(Self::Hdmi2),
            Input::Hdmi(3) => Some(Self::Hdmi3),
            Input::Hdmi(4) => Some(Self::Hdmi4),
            Input::Tv => Some(Self::Tv),
            Input::Usb(1) => Some(Self::Usb),
            Input::Other(name) => Self::from_name(name),
            _ => None,
        }
    }

    /// Maps an MDC source back to the shared input vocabulary.
    #[must_use]
    pub fn to_core_input(self) -> tv_rs_core::Input {
        use tv_rs_core::Input;

        match self {
            Self::Hdmi1 => Input::Hdmi(1),
            Self::Hdmi2 => Input::Hdmi(2),
            Self::Hdmi3 => Input::Hdmi(3),
            Self::Hdmi4 => Input::Hdmi(4),
            Self::Tv => Input::Tv,
            Self::Usb => Input::Usb(1),
            Self::DisplayPort1 => Input::Other("displayport1".to_string()),
            Self::DisplayPort2 => Input::Other("displayport2".to_string()),
            Self::DisplayPort3 => Input::Other("displayport3".to_string()),
            Self::Av => Input::Other("av".to_string()),
            Self::Component => Input::Other("component".to_string()),
            Self::Other(0x22) => Input::Hdmi(1),
            Self::Other(0x24) => Input::Hdmi(2),
            Self::Other(0x32) => Input::Hdmi(3),
            Self::Other(0x34) => Input::Hdmi(4),
            Self::Other(code) => Input::Other(format!("mdc_0x{code:02X}")),
        }
    }

    /// A stable label for logs and UI.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hdmi1 => "hdmi1",
            Self::Hdmi2 => "hdmi2",
            Self::Hdmi3 => "hdmi3",
            Self::Hdmi4 => "hdmi4",
            Self::DisplayPort1 => "displayport1",
            Self::DisplayPort2 => "displayport2",
            Self::DisplayPort3 => "displayport3",
            Self::Tv => "tv",
            Self::Usb => "usb",
            Self::Av => "av",
            Self::Component => "component",
            Self::Other(_) => "other",
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        let normalized: String = name
            .chars()
            .filter(|character| !matches!(character, '-' | '_' | ' '))
            .collect::<String>()
            .to_ascii_lowercase();

        match normalized.as_str() {
            "hdmi1" => Some(Self::Hdmi1),
            "hdmi2" => Some(Self::Hdmi2),
            "hdmi3" => Some(Self::Hdmi3),
            "hdmi4" => Some(Self::Hdmi4),
            "displayport1" | "dp1" => Some(Self::DisplayPort1),
            "displayport2" | "dp2" => Some(Self::DisplayPort2),
            "displayport3" | "dp3" => Some(Self::DisplayPort3),
            "tv" | "dtv" => Some(Self::Tv),
            "usb" => Some(Self::Usb),
            "av" => Some(Self::Av),
            "component" => Some(Self::Component),
            _ => None,
        }
    }
}

impl fmt::Display for InputSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Other(code) => write!(formatter, "mdc_0x{code:02X}"),
            other => formatter.write_str(other.as_str()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DisplayId, InputSource, MdcPower};

    #[test]
    fn display_ids_reject_the_response_header_value() {
        assert_eq!(DisplayId::new(0x00).unwrap(), DisplayId::DIRECT);
        assert_eq!(DisplayId::new(0xFE).unwrap(), DisplayId::BROADCAST);
        assert!(DisplayId::new(0xFF).is_err());
        assert!(DisplayId::BROADCAST.is_broadcast());
    }

    #[test]
    fn power_codes_round_trip() {
        for power in [MdcPower::Off, MdcPower::On, MdcPower::Reboot] {
            assert_eq!(MdcPower::from_code(power.code()), Some(power));
        }
        assert_eq!(MdcPower::from_code(0x7F), None);
    }

    #[test]
    fn input_mapping_is_lossless_for_non_settable_codes() {
        assert_eq!(InputSource::from_code(0x21), Some(InputSource::Hdmi1));
        assert_eq!(InputSource::from_code(0x22), Some(InputSource::Other(0x22)));
        assert_eq!(InputSource::from_code(0x00), None);
        assert_eq!(
            InputSource::from_code(0x22).unwrap().to_core_input(),
            tv_rs_core::Input::Hdmi(1)
        );
    }

    #[test]
    fn core_input_names_are_normalized() {
        assert_eq!(
            InputSource::from_core_input(&tv_rs_core::Input::Other("HDMI-2".into())),
            Some(InputSource::Hdmi2)
        );
        assert_eq!(
            InputSource::from_core_input(&tv_rs_core::Input::Other("dp 3".into())),
            Some(InputSource::DisplayPort3)
        );
        assert_eq!(
            InputSource::from_core_input(&tv_rs_core::Input::Hdmi(9)),
            None
        );
    }
}
