//! Constants and the ECP key enum for the Roku client.

use serde::{Deserialize, Serialize};

/// Default TCP port of the Roku ECP HTTP server on every Roku device.
pub const ROKU_ECP_PORT: u16 = 8060;

/// Path of the device information query endpoint (`GET`).
pub const ECP_DEVICE_INFO_PATH: &str = "/query/device-info";

/// Path of the installed applications query endpoint (`GET`).
pub const ECP_APPS_PATH: &str = "/query/apps";

/// Path prefix for the keypress endpoint (`POST`), followed by the key name.
pub const ECP_KEYPRESS_PATH_PREFIX: &str = "/keypress/";

/// What the owner must enable when a Roku refuses ECP writes.
///
/// Roku OS 14.1 added a user-facing gate; without naming it, a 403 reads as a broken
/// client rather than a setting to flip.
pub const CONTROL_BY_MOBILE_APPS_REMEDY: &str =
    "on the Roku: Settings -> System -> Advanced system settings -> Control by mobile apps -> Enabled";

/// Path prefix for the application launch endpoint (`POST`), followed by the app id.
pub const ECP_LAUNCH_PATH_PREFIX: &str = "/launch/";

/// A key that can be sent to a Roku device through the ECP keypress endpoint.
///
/// ECP keypress values are case-sensitive exact strings such as `"Home"`,
/// `"PowerOff"` and `"InstantReplay"`; [`RokuKey::as_str`] returns those exact
/// spellings. The serde representation mirrors them as well.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum RokuKey {
    /// Go to the home screen.
    Home,
    /// Rewind / scan backwards.
    Rev,
    /// Fast-forward / scan forwards.
    Fwd,
    /// Start or resume playback.
    Play,
    /// Confirm the highlighted item.
    Select,
    /// Move the highlight left.
    Left,
    /// Move the highlight right.
    Right,
    /// Move the highlight down.
    Down,
    /// Move the highlight up.
    Up,
    /// Go back one screen.
    Back,
    /// Jump back a few seconds (instant replay).
    InstantReplay,
    /// Show information about the current item.
    Info,
    /// Delete the character before the cursor.
    Backspace,
    /// Open the search screen.
    Search,
    /// Submit the current input.
    Enter,
    /// Power the device off.
    PowerOff,
    /// Power the device on.
    PowerOn,
    /// Turn the volume down.
    VolumeDown,
    /// Turn the volume up.
    VolumeUp,
    /// Mute or unmute the volume.
    VolumeMute,
    /// Tune to the next channel.
    ChannelUp,
    /// Tune to the previous channel.
    ChannelDown,
}

impl RokuKey {
    /// Returns the exact, case-sensitive ECP key string for this key.
    pub fn as_str(&self) -> &'static str {
        match self {
            RokuKey::Home => "Home",
            RokuKey::Rev => "Rev",
            RokuKey::Fwd => "Fwd",
            RokuKey::Play => "Play",
            RokuKey::Select => "Select",
            RokuKey::Left => "Left",
            RokuKey::Right => "Right",
            RokuKey::Down => "Down",
            RokuKey::Up => "Up",
            RokuKey::Back => "Back",
            RokuKey::InstantReplay => "InstantReplay",
            RokuKey::Info => "Info",
            RokuKey::Backspace => "Backspace",
            RokuKey::Search => "Search",
            RokuKey::Enter => "Enter",
            RokuKey::PowerOff => "PowerOff",
            RokuKey::PowerOn => "PowerOn",
            RokuKey::VolumeDown => "VolumeDown",
            RokuKey::VolumeUp => "VolumeUp",
            RokuKey::VolumeMute => "VolumeMute",
            RokuKey::ChannelUp => "ChannelUp",
            RokuKey::ChannelDown => "ChannelDown",
        }
    }
}

/// The `<power-mode>` values a Roku reports in `/query/device-info`.
///
/// Roku documents four values. `Headless` means the device is awake but no app is
/// drawing (typically after `PowerOff` on a streaming stick that cannot really be
/// switched off); `DisplayOff` means the panel is off while the device keeps
/// running — neither maps cleanly to "off", so they are preserved rather than
/// flattened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum RokuPowerMode {
    /// The device is on and drawing an app.
    PowerOn,
    /// Awake, but no app is drawing.
    Headless,
    /// Awake and ready, nothing playing.
    Ready,
    /// The panel is off while the device stays running.
    DisplayOff,
    /// The firmware reported something this crate does not model.
    #[default]
    Unknown,
}

impl RokuPowerMode {
    /// Parses the `<power-mode>` text. Unrecognised values become
    /// [`RokuPowerMode::Unknown`] rather than an error: a new firmware string must
    /// not break a probe.
    #[must_use]
    pub fn from_text(text: &str) -> Self {
        match text.trim() {
            "PowerOn" => Self::PowerOn,
            "Headless" => Self::Headless,
            "Ready" => Self::Ready,
            "DisplayOff" => Self::DisplayOff,
            _ => Self::Unknown,
        }
    }

    /// Returns the exact ECP spelling.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PowerOn => "PowerOn",
            Self::Headless => "Headless",
            Self::Ready => "Ready",
            Self::DisplayOff => "DisplayOff",
            Self::Unknown => "Unknown",
        }
    }

    /// Whether the device is drawing or able to draw an app.
    #[must_use]
    pub fn is_awake(&self) -> bool {
        matches!(self, Self::PowerOn | Self::Headless | Self::Ready)
    }
}

#[cfg(test)]
mod power_mode_tests {
    use super::RokuPowerMode;

    #[test]
    fn parses_the_four_documented_values() {
        assert_eq!(RokuPowerMode::from_text("PowerOn"), RokuPowerMode::PowerOn);
        assert_eq!(
            RokuPowerMode::from_text("Headless"),
            RokuPowerMode::Headless
        );
        assert_eq!(RokuPowerMode::from_text("Ready"), RokuPowerMode::Ready);
        assert_eq!(
            RokuPowerMode::from_text("DisplayOff"),
            RokuPowerMode::DisplayOff
        );
    }

    #[test]
    fn unknown_and_padded_values_do_not_break_a_probe() {
        assert_eq!(
            RokuPowerMode::from_text("  PowerOn\n"),
            RokuPowerMode::PowerOn
        );
        assert_eq!(
            RokuPowerMode::from_text("some 2027 mode"),
            RokuPowerMode::Unknown
        );
        assert_eq!(RokuPowerMode::default(), RokuPowerMode::Unknown);
    }

    #[test]
    fn round_trips_the_ecp_spelling() {
        for mode in [
            RokuPowerMode::PowerOn,
            RokuPowerMode::Headless,
            RokuPowerMode::Ready,
            RokuPowerMode::DisplayOff,
            RokuPowerMode::Unknown,
        ] {
            assert_eq!(RokuPowerMode::from_text(mode.as_str()), mode);
        }
    }

    #[test]
    fn awake_covers_headless_and_ready_but_not_display_off() {
        assert!(RokuPowerMode::PowerOn.is_awake());
        assert!(RokuPowerMode::Headless.is_awake());
        assert!(RokuPowerMode::Ready.is_awake());
        assert!(!RokuPowerMode::DisplayOff.is_awake());
        assert!(!RokuPowerMode::Unknown.is_awake());
    }
}
