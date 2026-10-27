//! Brand IR profiles and TOML configuration persistence.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use tv_rs_core::Key;

use crate::error::IrError;
use crate::protocol::{IrProtocol, IrSignal};

/// Profile configuration mapping remote keys to brand IR command codes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IrProfile {
    /// Human-readable profile name.
    pub name: String,
    /// Targeted device brand (e.g., "samsung", "lg", "sony", "amlogic").
    pub brand: String,
    /// IR transmission protocol.
    pub protocol: IrProtocol,
    /// Carrier frequency in kHz (e.g., 38, 36, 40, 56).
    pub carrier_khz: u32,
    /// Device / customer address.
    pub address: u16,
    /// Map of normalized key names to hex command codes.
    pub keys: HashMap<String, u32>,
}

impl IrProfile {
    /// Builds a new profile with the given brand and protocol defaults.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        brand: impl Into<String>,
        protocol: IrProtocol,
        address: u16,
    ) -> Self {
        Self {
            name: name.into(),
            brand: brand.into(),
            protocol,
            carrier_khz: protocol.default_carrier_khz(),
            address,
            keys: HashMap::new(),
        }
    }

    /// Attaches a key mapping.
    #[must_use]
    pub fn with_key(mut self, key: impl Into<String>, command: u32) -> Self {
        self.keys.insert(key.into(), command);
        self
    }

    /// Looks up the hex command code for a given [`Key`].
    #[must_use]
    pub fn command_for_key(&self, key: Key) -> Option<u32> {
        let name = key.as_str();
        self.keys.get(name).copied().or_else(|| match key {
            Key::Power => self.keys.get("power").copied(),
            Key::PowerOff => self
                .keys
                .get("power_off")
                .copied()
                .or_else(|| self.keys.get("power").copied()),
            Key::VolumeUp => self
                .keys
                .get("volume_up")
                .copied()
                .or_else(|| self.keys.get("vol_up").copied()),
            Key::VolumeDown => self
                .keys
                .get("volume_down")
                .copied()
                .or_else(|| self.keys.get("vol_down").copied()),
            Key::Mute => self.keys.get("mute").copied(),
            Key::Up => self.keys.get("up").copied(),
            Key::Down => self.keys.get("down").copied(),
            Key::Left => self.keys.get("left").copied(),
            Key::Right => self.keys.get("right").copied(),
            Key::Select => self
                .keys
                .get("select")
                .copied()
                .or_else(|| self.keys.get("enter").copied())
                .or_else(|| self.keys.get("ok").copied()),
            Key::Back => self
                .keys
                .get("back")
                .copied()
                .or_else(|| self.keys.get("return").copied()),
            Key::Home => self.keys.get("home").copied(),
            Key::Menu => self.keys.get("menu").copied(),
            _ => None,
        })
    }

    /// Generates an [`IrSignal`] for a specific [`Key`].
    ///
    /// # Errors
    ///
    /// Returns [`IrError::Protocol`] if the key is not mapped in this profile or encoding fails.
    pub fn signal_for_key(&self, key: Key) -> Result<IrSignal, IrError> {
        let cmd = self.command_for_key(key).ok_or_else(|| {
            IrError::Protocol(format!(
                "key '{}' not mapped in profile '{}'",
                key.as_str(),
                self.name
            ))
        })?;

        IrSignal::encode(self.protocol, self.address, cmd, Some(self.carrier_khz))
    }

    /// Serializes the profile to a TOML string.
    ///
    /// # Errors
    ///
    /// Returns [`IrError::Profile`] if serialization fails.
    pub fn to_toml(&self) -> Result<String, IrError> {
        toml::to_string_pretty(self).map_err(|e| IrError::Profile(e.to_string()))
    }

    /// Deserializes an `IrProfile` from a TOML string.
    ///
    /// # Errors
    ///
    /// Returns [`IrError::Profile`] if parsing fails.
    pub fn from_toml(content: &str) -> Result<Self, IrError> {
        toml::from_str(content).map_err(|e| IrError::Profile(e.to_string()))
    }

    /// Writes the profile to a TOML file.
    ///
    /// # Errors
    ///
    /// Returns [`IrError::Profile`] on I/O or serialization failure.
    pub fn save_to_file(&self, path: impl AsRef<Path>) -> Result<(), IrError> {
        let content = self.to_toml()?;
        fs::write(path.as_ref(), content).map_err(|e| IrError::Profile(e.to_string()))
    }

    /// Reads an `IrProfile` from a TOML file.
    ///
    /// # Errors
    ///
    /// Returns [`IrError::Profile`] on I/O or parsing failure.
    pub fn load_from_file(path: impl AsRef<Path>) -> Result<Self, IrError> {
        let content =
            fs::read_to_string(path.as_ref()).map_err(|e| IrError::Profile(e.to_string()))?;
        Self::from_toml(&content)
    }
}

/// Returns standard default Samsung TV IR profile (NEC 38 kHz).
#[must_use]
pub fn default_samsung_profile() -> IrProfile {
    IrProfile::new("Samsung TV", "samsung", IrProtocol::Nec, 0x0707)
        .with_key("power", 0x02)
        .with_key("power_off", 0x99)
        .with_key("volume_up", 0x07)
        .with_key("volume_down", 0x0B)
        .with_key("mute", 0x0F)
        .with_key("up", 0x60)
        .with_key("down", 0x61)
        .with_key("left", 0x65)
        .with_key("right", 0x62)
        .with_key("select", 0x68)
        .with_key("back", 0x58)
        .with_key("home", 0x79)
        .with_key("menu", 0x1A)
}

/// Returns standard default LG TV IR profile (NEC 38 kHz).
#[must_use]
pub fn default_lg_profile() -> IrProfile {
    IrProfile::new("LG TV", "lg", IrProtocol::Nec, 0x0404)
        .with_key("power", 0x08)
        .with_key("power_off", 0xC7)
        .with_key("volume_up", 0x02)
        .with_key("volume_down", 0x03)
        .with_key("mute", 0x09)
        .with_key("up", 0x40)
        .with_key("down", 0x41)
        .with_key("left", 0x07)
        .with_key("right", 0x06)
        .with_key("select", 0x44)
        .with_key("back", 0x28)
        .with_key("home", 0x21)
        .with_key("menu", 0x43)
}

/// Returns standard default Sony TV IR profile (SIRC 40 kHz).
#[must_use]
pub fn default_sony_profile() -> IrProfile {
    IrProfile::new("Sony TV", "sony", IrProtocol::Sirc, 0x01)
        .with_key("power", 0x15)
        .with_key("power_off", 0x2E)
        .with_key("volume_up", 0x12)
        .with_key("volume_down", 0x13)
        .with_key("mute", 0x14)
        .with_key("up", 0x09)
        .with_key("down", 0x0A)
        .with_key("left", 0x0D)
        .with_key("right", 0x0C)
        .with_key("select", 0x0B)
        .with_key("back", 0x23)
        .with_key("home", 0x60)
        .with_key("menu", 0x60)
}

/// Returns standard default Philips TV IR profile (RC5 36 kHz).
#[must_use]
pub fn default_philips_profile() -> IrProfile {
    IrProfile::new("Philips TV", "philips", IrProtocol::Rc5, 0x00)
        .with_key("power", 0x0C)
        .with_key("power_off", 0x3D)
        .with_key("volume_up", 0x10)
        .with_key("volume_down", 0x11)
        .with_key("mute", 0x0D)
        .with_key("up", 0x10)
        .with_key("down", 0x11)
        .with_key("left", 0x15)
        .with_key("right", 0x16)
        .with_key("select", 0x17)
        .with_key("back", 0x0A)
        .with_key("home", 0x20)
}

/// Returns standard default RCA TV IR profile (RCA 38 kHz).
#[must_use]
pub fn default_rca_profile() -> IrProfile {
    IrProfile::new("RCA TV", "rca", IrProtocol::Rca, 0x0F)
        .with_key("power", 0x08)
        .with_key("volume_up", 0x02)
        .with_key("volume_down", 0x03)
        .with_key("mute", 0x09)
        .with_key("up", 0x1A)
        .with_key("down", 0x1B)
        .with_key("left", 0x1C)
        .with_key("right", 0x1D)
        .with_key("select", 0x25)
        .with_key("back", 0x2B)
}

/// Returns standard default Amlogic TV Box IR profile (NEC 38 kHz).
#[must_use]
pub fn default_amlogic_profile() -> IrProfile {
    IrProfile::new("Amlogic TV Box", "amlogic", IrProtocol::Amlogic, 0x4040)
        .with_key("power", 0x14)
        .with_key("power_off", 0x14)
        .with_key("volume_up", 0x13)
        .with_key("volume_down", 0x17)
        .with_key("mute", 0x50)
        .with_key("up", 0x03)
        .with_key("down", 0x02)
        .with_key("left", 0x0E)
        .with_key("right", 0x1A)
        .with_key("select", 0x07)
        .with_key("back", 0x51)
        .with_key("home", 0x49)
        .with_key("menu", 0x48)
}

/// Returns all pre-built brand IR profiles.
#[must_use]
pub fn all_default_profiles() -> Vec<IrProfile> {
    vec![
        default_samsung_profile(),
        default_lg_profile(),
        default_sony_profile(),
        default_amlogic_profile(),
        default_philips_profile(),
        default_rca_profile(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_toml_roundtrip() {
        let profile = default_samsung_profile();
        let toml_str = profile.to_toml().unwrap();
        let loaded = IrProfile::from_toml(&toml_str).unwrap();
        assert_eq!(profile, loaded);
    }

    #[test]
    fn command_lookup_for_key() {
        let profile = default_lg_profile();
        assert_eq!(profile.command_for_key(Key::Power), Some(0x08));
        assert_eq!(profile.command_for_key(Key::VolumeUp), Some(0x02));
        assert!(profile.signal_for_key(Key::Power).is_ok());
    }
}
