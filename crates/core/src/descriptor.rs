//! Identity, operating system and power vocabulary.

use std::fmt;

use crate::error::TvError;

/// A stable identifier for one television.
///
/// Stable means "survives a DHCP lease change": a device keeps its id when its
/// address moves. Sources, in order of preference: the UPnP `UDN`, the Roku
/// `device-id`, then a MAC address. An id is an opaque string — do not parse it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DeviceId(String);

impl DeviceId {
    /// Wraps an already-stable identifier.
    ///
    /// # Errors
    ///
    /// Returns [`TvError::Protocol`] when the identifier is empty after trimming:
    /// an empty id would silently merge every device in a fleet.
    pub fn new(value: impl Into<String>) -> Result<Self, TvError> {
        let value = value.into();
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(TvError::protocol("device id must not be empty"));
        }
        Ok(Self(trimmed.to_owned()))
    }

    /// A placeholder identity for a device that discovery could not name.
    ///
    /// Prefer a UDN, a `device-id` or a MAC. This exists so a handle built from an
    /// address alone (a static-IP install) can still implement the trait without a
    /// panic path.
    #[must_use]
    pub fn anonymous() -> Self {
        Self("anonymous".to_string())
    }

    /// The identifier as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The platform a device runs, as far as discovery can tell.
///
/// SSDP carries no authoritative vendor field, so this is a classification, not a
/// fact. [`DeviceOs::Unknown`] is a legitimate answer and must not be guessed into
/// one of the others.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum DeviceOs {
    /// Samsung's Tizen remote-channel firmware.
    Tizen,
    /// LG's webOS SSAP firmware.
    #[cfg_attr(feature = "serde", serde(rename = "webos"))]
    WebOs,
    /// Roku OS, whether a TV or a player.
    Roku,
    /// Samsung commercial firmware driven by MDC over RJ45 or RS-232.
    ///
    /// A different control surface from the consumer Tizen channel: no pairing, no
    /// WebSocket, and a display id instead of a remote.
    #[cfg_attr(feature = "serde", serde(rename = "samsung_mdc"))]
    SamsungMdc,
    /// Philips commercial firmware driven by SICP over TCP or RS-232.
    #[cfg_attr(feature = "serde", serde(rename = "philips_sicp"))]
    PhilipsSicp,
    /// Google Cast or Chromecast.
    #[cfg_attr(feature = "serde", serde(rename = "googlecast"))]
    GoogleCast,
    /// Discovered, not identified.
    #[default]
    #[cfg_attr(feature = "serde", serde(rename = "unknown"))]
    Unknown,
}

impl DeviceOs {
    /// A short, stable label for logs and UI.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Tizen => "tizen",
            Self::WebOs => "webos",
            Self::Roku => "roku",
            Self::SamsungMdc => "samsung_mdc",
            Self::PhilipsSicp => "philips_sicp",
            Self::Unknown => "unknown",
            Self::GoogleCast => "googlecast",
        }
    }
}

impl fmt::Display for DeviceOs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Everything known about a device before a single command is sent.
///
/// Every optional field is optional because discovery genuinely does not always
/// answer: a Roku player reports no panel, a Samsung set may omit firmware.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceDescriptor {
    /// Stable identifier.
    pub id: DeviceId,
    /// Classified platform.
    pub os: DeviceOs,
    /// Network address the device was found at (host or host:port).
    pub address: String,
    /// User-assigned or advertised name.
    pub name: Option<String>,
    /// Model name or number.
    pub model: Option<String>,
    /// Firmware/software version, when advertised.
    pub firmware: Option<String>,
    /// MAC address, when advertised — required for Wake-on-LAN.
    pub mac: Option<String>,
}

impl DeviceDescriptor {
    /// Builds a descriptor with only the fields discovery always provides.
    #[must_use]
    pub fn new(id: DeviceId, os: DeviceOs, address: impl Into<String>) -> Self {
        Self {
            id,
            os,
            address: address.into(),
            name: None,
            model: None,
            firmware: None,
            mac: None,
        }
    }

    /// Sets the advertised device name.
    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Sets the model.
    #[must_use]
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Sets the firmware version.
    #[must_use]
    pub fn with_firmware(mut self, firmware: impl Into<String>) -> Self {
        self.firmware = Some(firmware.into());
        self
    }

    /// Sets the MAC address.
    #[must_use]
    pub fn with_mac(mut self, mac: impl Into<String>) -> Self {
        self.mac = Some(mac.into());
        self
    }

    /// The label a UI should show: name, then model, then address.
    #[must_use]
    pub fn label(&self) -> &str {
        self.name
            .as_deref()
            .or(self.model.as_deref())
            .unwrap_or(&self.address)
    }
}

/// What a television is doing right now, as far as it will say.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum PowerState {
    /// Awake and able to accept commands.
    On,
    /// The network stack answers but the application layer is down — the usual
    /// state of a TV that is "off" but reachable.
    Standby,
    /// Nothing answered.
    Off,
    /// The device answered in a way this model does not cover.
    Unknown,
}

impl PowerState {
    /// Whether commands can be sent right now.
    #[must_use]
    pub fn is_awake(&self) -> bool {
        matches!(self, Self::On)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_id_rejects_empty_values() {
        assert!(DeviceId::new("   ").is_err());
        assert_eq!(DeviceId::new("  udn:abc  ").unwrap().as_str(), "udn:abc");
    }

    #[test]
    fn label_falls_back_from_name_to_model_to_address() {
        let id = DeviceId::new("id").unwrap();
        let mut descriptor = DeviceDescriptor::new(id, DeviceOs::Roku, "192.168.1.40");
        assert_eq!(descriptor.label(), "192.168.1.40");
        descriptor = descriptor.with_model("Roku Ultra");
        assert_eq!(descriptor.label(), "Roku Ultra");
        descriptor = descriptor.with_name("Living Room");
        assert_eq!(descriptor.label(), "Living Room");
    }

    #[test]
    fn unknown_is_the_default_platform() {
        assert_eq!(DeviceOs::default(), DeviceOs::Unknown);
        assert_eq!(DeviceOs::Unknown.as_str(), "unknown");
    }

    #[test]
    fn only_on_counts_as_awake() {
        assert!(PowerState::On.is_awake());
        assert!(!PowerState::Standby.is_awake());
        assert!(!PowerState::Off.is_awake());
        assert!(!PowerState::Unknown.is_awake());
    }
}
