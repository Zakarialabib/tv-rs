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
