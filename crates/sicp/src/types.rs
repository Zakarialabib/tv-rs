//! Domain types for Philips SICP display control.

use std::fmt;

use tv_rs_core::Input;

use crate::error::SicpError;

/// A display address on a SICP bus.
///
/// `0x01` addresses display 1 directly (the single-display default).
/// `0x00` (or `0xFF`) is a broadcast address targeting all displays on the line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DisplayId(u8);

impl DisplayId {
    /// Default display ID for a single display (`0x01`).
    pub const DIRECT: Self = Self(1);

    /// Broadcast display ID (`0x00`).
    pub const BROADCAST: Self = Self(0);

    /// Builds a display ID.
    ///
    /// # Errors
    ///
    /// Returns [`SicpError::InvalidDisplayId`] if the ID is outside the valid range (0–255).
    pub fn new(value: u8) -> Result<Self, SicpError> {
        Ok(Self(value))
    }

    /// The raw byte value.
    #[must_use]
    pub const fn value(self) -> u8 {
        self.0
    }

    /// Whether this is a broadcast address (`0x00` or `0xFF`).
    #[must_use]
    pub const fn is_broadcast(self) -> bool {
        self.0 == 0 || self.0 == 0xFF
    }
}

impl Default for DisplayId {
    fn default() -> Self {
        Self::DIRECT
    }
}

impl fmt::Display for DisplayId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Power state of a Philips SICP display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SicpPower {
    /// Standby / Power Saver mode (`0x00`).
    Standby,
    /// Fully Powered Off (`0x01`).
    Off,
    /// Powered On (`0x02`).
    On,
}

impl SicpPower {
    /// The wire code for this power state.
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::Standby => 0x00,
            Self::Off => 0x01,
            Self::On => 0x02,
        }
    }

    /// Converts a wire code to `SicpPower`.
    #[must_use]
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            0x00 => Some(Self::Standby),
            0x01 => Some(Self::Off),
            0x02 => Some(Self::On),
            _ => None,
        }
    }
}

/// An input source on a Philips SICP display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SicpInput {
    /// VGA / Analog (`0x01`).
    Vga,
    /// DVI (`0x02`).
    Dvi,
    /// HDMI 1 (`0x04`).
    Hdmi1,
    /// HDMI 2 (`0x05`).
    Hdmi2,
    /// DisplayPort (`0x06`).
    DisplayPort,
    /// TV Tuner (`0x07`).
    Tv,
    /// Vendor or unmodeled input source code.
    Other(u8),
}

impl SicpInput {
    /// The wire code for this input source.
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::Vga => 0x01,
            Self::Dvi => 0x02,
            Self::Hdmi1 => 0x04,
            Self::Hdmi2 => 0x05,
            Self::DisplayPort => 0x06,
            Self::Tv => 0x07,
            Self::Other(code) => code,
        }
    }

    /// Converts a wire code to `SicpInput`.
    #[must_use]
    pub const fn from_code(code: u8) -> Self {
        match code {
            0x01 => Self::Vga,
            0x02 => Self::Dvi,
            0x04 => Self::Hdmi1,
            0x05 => Self::Hdmi2,
            0x06 => Self::DisplayPort,
            0x07 => Self::Tv,
            other => Self::Other(other),
        }
    }

    /// Maps to vendor-neutral [`tv_rs_core::Input`].
    #[must_use]
    pub fn to_core_input(self) -> Input {
        match self {
            Self::Hdmi1 => Input::Hdmi(1),
            Self::Hdmi2 => Input::Hdmi(2),
            Self::Tv => Input::Tv,
            Self::Vga => Input::Other("VGA".to_string()),
            Self::Dvi => Input::Other("DVI".to_string()),
            Self::DisplayPort => Input::Other("DisplayPort".to_string()),
            Self::Other(code) => Input::Other(format!("Input_{code:#04X}")),
        }
    }

    /// Converts from vendor-neutral [`tv_rs_core::Input`].
    #[must_use]
    pub fn from_core_input(input: &Input) -> Option<Self> {
        match input {
            Input::Hdmi(1) => Some(Self::Hdmi1),
            Input::Hdmi(2) => Some(Self::Hdmi2),
            Input::Tv => Some(Self::Tv),
            Input::Other(name) => match name.to_ascii_uppercase().as_str() {
                "VGA" => Some(Self::Vga),
                "DVI" => Some(Self::Dvi),
                "DP" | "DISPLAYPORT" => Some(Self::DisplayPort),
                _ => None,
            },
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_id_creation_and_broadcast() {
        let direct = DisplayId::new(1).unwrap();
        assert!(!direct.is_broadcast());
        assert_eq!(direct.value(), 1);

        let broadcast = DisplayId::BROADCAST;
        assert!(broadcast.is_broadcast());
        assert_eq!(broadcast.value(), 0);
    }

    #[test]
    fn power_codes() {
        assert_eq!(SicpPower::On.code(), 0x02);
        assert_eq!(SicpPower::from_code(0x02), Some(SicpPower::On));
        assert_eq!(SicpPower::from_code(0x00), Some(SicpPower::Standby));
        assert_eq!(SicpPower::from_code(0x99), None);
    }

    #[test]
    fn input_conversion() {
        assert_eq!(SicpInput::Hdmi1.to_core_input(), Input::Hdmi(1));
        assert_eq!(
            SicpInput::from_core_input(&Input::Hdmi(2)),
            Some(SicpInput::Hdmi2)
        );
        assert_eq!(
            SicpInput::from_core_input(&Input::Other("VGA".to_string())),
            Some(SicpInput::Vga)
        );
    }
}
