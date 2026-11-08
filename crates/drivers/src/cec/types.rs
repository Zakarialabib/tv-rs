//! Logical address and command models for HDMI-CEC.

use std::fmt;
use crate::cec::error::CecError;

/// A logical address on the CEC bus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CecAddress {
    /// The television. Always address 0.
    Tv,
    /// Playback device 1 — the usual address for the first source.
    Playback1,
    /// Playback device 2.
    Playback2,
    /// Audio system.
    AudioSystem,
    /// Everyone on the bus.
    Broadcast,
}

impl CecAddress {
    /// The numeric logical address.
    #[must_use]
    pub fn as_u8(&self) -> u8 {
        match self {
            Self::Tv => 0,
            Self::Playback1 => 4,
            Self::Playback2 => 8,
            Self::AudioSystem => 5,
            Self::Broadcast => 15,
        }
    }
}

impl fmt::Display for CecAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_u8())
    }
}

impl TryFrom<u8> for CecAddress {
    type Error = CecError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Tv),
            4 => Ok(Self::Playback1),
            8 => Ok(Self::Playback2),
            5 => Ok(Self::AudioSystem),
            15 => Ok(Self::Broadcast),
            _ => Err(CecError::Rejected(format!(
                "unsupported or invalid cec logical address: {value}"
            ))),
        }
    }
}

impl TryFrom<&str> for CecAddress {
    type Error = CecError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let trimmed = value.trim();
        if let Ok(num) = trimmed.parse::<u8>() {
            return Self::try_from(num);
        }
        if trimmed.starts_with("0x") || trimmed.starts_with("0X") {
            if let Ok(num) = u8::from_str_radix(&trimmed[2..], 16) {
                return Self::try_from(num);
            }
        }
        match trimmed.to_ascii_lowercase().as_str() {
            "tv" => Ok(Self::Tv),
            "playback1" | "playback_1" => Ok(Self::Playback1),
            "playback2" | "playback_2" => Ok(Self::Playback2),
            "audiosystem" | "audio_system" | "audio" => Ok(Self::AudioSystem),
            "broadcast" => Ok(Self::Broadcast),
            _ => Err(CecError::Rejected(format!(
                "invalid cec address string: '{value}'"
            ))),
        }
    }
}

/// A structured CEC command line that can be executed by a CEC adapter or `cec-client`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CecCommand {
    /// Powers the addressed device on (e.g. `on 0`).
    PowerOn(CecAddress),
    /// Puts the addressed device into standby (e.g. `standby 0`).
    Standby(CecAddress),
    /// Broadcasts active source for physical HDMI address (e.g. `tx 0F:82:10:00`).
    SetActiveSource(u16),
    /// A custom hex frame (e.g. `tx 0F:82:10:00` or `10:00`).
    CustomHexFrame(String),
    /// A raw command string for `cec-client`.
    Raw(String),
}

impl CecCommand {
    /// Validates the command string representation for security and grammar compliance.
    ///
    /// # Errors
    ///
    /// Returns [`CecError::Rejected`] if the command is empty or contains control
    /// characters or newlines.
    pub fn validate(&self) -> Result<(), CecError> {
        let cmd_str = self.to_string();
        let trimmed = cmd_str.trim();
        if trimmed.is_empty() || !cmd_str.chars().all(|c| c.is_ascii_graphic() || c == ' ') {
            return Err(CecError::Rejected(format!(
                "invalid cec command: '{cmd_str}' contains illegal control characters, newlines, or is empty"
            )));
        }
        Ok(())
    }
}

impl fmt::Display for CecCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PowerOn(addr) => write!(f, "on {addr}"),
            Self::Standby(addr) => write!(f, "standby {addr}"),
            Self::SetActiveSource(phys) => {
                let high = (phys >> 8) as u8;
                let low = (phys & 0x00ff) as u8;
                write!(f, "tx 0F:82:{high:02X}:{low:02X}")
            }
            Self::CustomHexFrame(frame) => {
                let trimmed = frame.trim();
                if trimmed.to_ascii_lowercase().starts_with("tx ") {
                    write!(f, "{trimmed}")
                } else {
                    write!(f, "tx {trimmed}")
                }
            }
            Self::Raw(raw) => write!(f, "{}", raw.trim()),
        }
    }
}

/// Parses a physical address from a string (e.g. "1.0.0.0" or "0x1000").
pub fn parse_physical_address(s: &str) -> Result<u16, CecError> {
    let clean = s.replace([':', '.'], "");
    let hex_str = clean
        .strip_prefix("0x")
        .or_else(|| clean.strip_prefix("0X"))
        .unwrap_or(&clean);
    u16::from_str_radix(hex_str, 16)
        .map_err(|_| CecError::Rejected(format!("invalid physical address format: '{s}'")))
}

/// Parses active source physical address from a hex frame if matching.
pub fn parse_active_source_frame(hex_part: &str) -> Option<u16> {
    let clean = hex_part.replace([':', ' '], "");
    if clean.len() == 8
        && clean.get(0..4).map(|s| s.to_ascii_uppercase()) == Some("0F82".to_string())
    {
        u16::from_str_radix(&clean[4..8], 16).ok()
    } else {
        None
    }
}

/// Checks if a string is a valid CEC hex frame.
pub fn is_valid_hex_frame(s: &str) -> bool {
    let clean = s.trim();
    if clean.is_empty() {
        return false;
    }
    let hex_part = clean.strip_prefix("tx ").unwrap_or(clean);
    let stripped = hex_part.replace([':', ' '], "");
    !stripped.is_empty()
        && stripped.len().is_multiple_of(2)
        && stripped.chars().all(|c| c.is_ascii_hexdigit())
}

impl TryFrom<&str> for CecCommand {
    type Error = CecError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let trimmed = value.trim();
        if trimmed.is_empty() || !value.chars().all(|c| c.is_ascii_graphic() || c == ' ') {
            return Err(CecError::Rejected(format!(
                "invalid cec command: '{value}' contains illegal control characters, newlines, or is empty"
            )));
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.is_empty() {
            return Err(CecError::Rejected("empty cec command".to_string()));
        }

        match parts[0].to_ascii_lowercase().as_str() {
            "on" => {
                if parts.len() == 2 {
                    let addr = CecAddress::try_from(parts[1])?;
                    Ok(CecCommand::PowerOn(addr))
                } else {
                    Err(CecError::Rejected(format!(
                        "invalid 'on' command format: '{trimmed}'"
                    )))
                }
            }
            "standby" | "st" => {
                if parts.len() == 2 {
                    let addr = CecAddress::try_from(parts[1])?;
                    Ok(CecCommand::Standby(addr))
                } else {
                    Err(CecError::Rejected(format!(
                        "invalid 'standby' command format: '{trimmed}'"
                    )))
                }
            }
            "as" | "active_source" => {
                if parts.len() == 1 {
                    Ok(CecCommand::SetActiveSource(0x1000))
                } else if parts.len() == 2 {
                    let phys = parse_physical_address(parts[1])?;
                    Ok(CecCommand::SetActiveSource(phys))
                } else {
                    Err(CecError::Rejected(format!(
                        "invalid active source command format: '{trimmed}'"
                    )))
                }
            }
            "tx" => {
                if parts.len() > 1 {
                    let hex_part = parts[1..].join(" ");
                    if let Some(phys) = parse_active_source_frame(&hex_part) {
                        Ok(CecCommand::SetActiveSource(phys))
                    } else if is_valid_hex_frame(&hex_part) {
                        Ok(CecCommand::CustomHexFrame(trimmed.to_string()))
                    } else {
                        Err(CecError::Rejected(format!(
                            "invalid hex frame payload in 'tx': '{trimmed}'"
                        )))
                    }
                } else {
                    Err(CecError::Rejected("missing payload after 'tx'".to_string()))
                }
            }
            _ => {
                if is_valid_hex_frame(trimmed) {
                    if let Some(phys) = parse_active_source_frame(trimmed) {
                        Ok(CecCommand::SetActiveSource(phys))
                    } else {
                        Ok(CecCommand::CustomHexFrame(trimmed.to_string()))
                    }
                } else {
                    Ok(CecCommand::Raw(trimmed.to_string()))
                }
            }
        }
    }
}

impl TryFrom<String> for CecCommand {
    type Error = CecError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        CecCommand::try_from(value.as_str())
    }
}
