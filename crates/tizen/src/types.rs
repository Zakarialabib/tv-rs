//! Constants and enums for the Samsung Tizen protocol.

use serde::{Deserialize, Serialize};

use crate::error::{Result, TizenError};

/// Port of the legacy, unencrypted Tizen WebSocket remote channel.
pub const TIZEN_WS_PORT: u16 = 8001;

/// Port of the modern, TLS-wrapped Tizen WebSocket remote channel.
pub const TIZEN_WSS_PORT: u16 = 8002;

/// Port of the Tizen DIAL (Discovery And Launch) HTTP service.
pub const TIZEN_DIAL_PORT: u16 = 8080;

/// Path of the Tizen remote-control WebSocket channel.
pub const TIZEN_REMOTE_WS_PATH: &str = "/api/v2/channels/samsung.remote.control";

/// Path of the DIAL device description document.
pub const TIZEN_DIAL_DESC_PATH: &str = "/ssdp/device-desc.xml";

/// Path of the unauthenticated device-information document.
///
/// It shares port 8001 with the remote channel's WebSocket, at a different path — and
/// unlike the channel, this read needs no pairing and no token. Where the MAC it
/// carries comes from, and its traps: `docs/01-playbook.md` §3.
pub const TIZEN_DEVICE_INFO_PATH: &str = "/api/v2/";

/// Prefix for DIAL app-launch requests (`POST {prefix}{app_id}`).
pub const TIZEN_DIAL_LAUNCH_PREFIX: &str = "/ws/apps/";

/// Application identifier this crate advertises to the TV.
pub const TIZEN_APP_ID: &str = "tv-rs";

/// Value of the `Cmd` field used for remote key clicks.
pub const TIZEN_CMD_CLICK: &str = "Click";

/// Value of the `TypeOfRemote` field used for remote key clicks.
pub const TIZEN_REMOTE_TYPE: &str = "SendRemoteKey";

/// `TypeOfRemote` for the frame that types a string, rather than pressing a key.
pub const TIZEN_REMOTE_TYPE_TEXT: &str = "SendInputString";

/// `DataOfCmd` in the text frame: it tells the set that `Cmd` is base64.
pub const TIZEN_CMD_BASE64: &str = "base64";

/// Launch mode for the remote channel's `ed.apps.launch` event.
///
/// Tizen exposes two modes: [`NativeLaunch`](Self::NativeLaunch) starts a Tizen
/// application as-is, while [`DeepLink`](Self::DeepLink) asks the TV to resolve
/// the app through its deep-link handler (the mode most streaming apps use).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TizenLaunchAction {
    /// Start the application natively.
    NativeLaunch,
    /// Launch through the app's deep-link handler.
    DeepLink,
}

impl TizenLaunchAction {
    /// The exact wire value sent as `action_type`.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NativeLaunch => "NATIVE_LAUNCH",
            Self::DeepLink => "DEEP_LINK",
        }
    }
}

/// Validates a raw Samsung `KEY_*` code.
///
/// The TV key set varies by model and firmware, so this checks the wire shape
/// (`KEY_` + 1–35 uppercase ASCII/digit/underscore characters) rather than
/// maintaining an exhaustive allow-list. Use it before sending a code that is
/// not one of the typed [`TizenRemoteKey`] variants.
pub fn validate_remote_key_code(code: &str) -> Result<()> {
    let valid = code.len() >= 5
        && code.len() <= 40
        && code.starts_with("KEY_")
        && code.chars().all(|character| {
            character.is_ascii_uppercase() || character.is_ascii_digit() || character == '_'
        });
    if valid {
        Ok(())
    } else {
        Err(TizenError::InvalidKey(code.to_string()))
    }
}

/// A remote-control key understood by the Tizen remote channel.
///
/// The wire value is the Samsung key code returned by [`TizenRemoteKey::as_str`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TizenRemoteKey {
    /// Power toggle.
    Power,
    /// Power off.
    PowerOff,
    /// Home / smart hub.
    Home,
    /// Directional up.
    Up,
    /// Directional down.
    Down,
    /// Directional left.
    Left,
    /// Directional right.
    Right,
    /// Confirm / select.
    Enter,
    /// Back / return.
    Return,
    /// Volume up.
    VolumeUp,
    /// Volume down.
    VolumeDown,
    /// Mute toggle.
    Mute,
    /// Next channel.
    ChannelUp,
    /// Previous channel.
    ChannelDown,
    /// Main menu.
    Menu,
    /// Input source.
    Source,
    /// Play.
    Play,
    /// Pause.
    Pause,
    /// Stop.
    Stop,
    /// Rewind.
    Rewind,
    /// Fast forward.
    FastForward,
}

impl TizenRemoteKey {
    /// Maps an exact Samsung key code to a typed variant.
    ///
    /// Returns `None` for model-specific codes that are valid on the wire but
    /// not represented by this enum; send those with
    /// [`validate_remote_key_code`] plus
    /// [`TizenSession::send_key_code`](crate::TizenSession::send_key_code).
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        let key = match code {
            "KEY_POWER" => Self::Power,
            "KEY_POWEROFF" => Self::PowerOff,
            "KEY_HOME" => Self::Home,
            "KEY_UP" => Self::Up,
            "KEY_DOWN" => Self::Down,
            "KEY_LEFT" => Self::Left,
            "KEY_RIGHT" => Self::Right,
            "KEY_ENTER" => Self::Enter,
            "KEY_RETURN" => Self::Return,
            "KEY_VOLUP" => Self::VolumeUp,
            "KEY_VOLDOWN" => Self::VolumeDown,
            "KEY_MUTE" => Self::Mute,
            "KEY_CHUP" => Self::ChannelUp,
            "KEY_CHDOWN" => Self::ChannelDown,
            "KEY_MENU" => Self::Menu,
            "KEY_SOURCE" => Self::Source,
            "KEY_PLAY" => Self::Play,
            "KEY_PAUSE" => Self::Pause,
            "KEY_STOP" => Self::Stop,
            "KEY_REWIND" => Self::Rewind,
            "KEY_FF" => Self::FastForward,
            _ => return None,
        };
        Some(key)
    }

    /// Returns the exact Samsung key code sent as `DataOfCmd`.
    pub fn as_str(&self) -> &'static str {
        match self {
            TizenRemoteKey::Power => "KEY_POWER",
            TizenRemoteKey::PowerOff => "KEY_POWEROFF",
            TizenRemoteKey::Home => "KEY_HOME",
            TizenRemoteKey::Up => "KEY_UP",
            TizenRemoteKey::Down => "KEY_DOWN",
            TizenRemoteKey::Left => "KEY_LEFT",
            TizenRemoteKey::Right => "KEY_RIGHT",
            TizenRemoteKey::Enter => "KEY_ENTER",
            TizenRemoteKey::Return => "KEY_RETURN",
            TizenRemoteKey::VolumeUp => "KEY_VOLUP",
            TizenRemoteKey::VolumeDown => "KEY_VOLDOWN",
            TizenRemoteKey::Mute => "KEY_MUTE",
            TizenRemoteKey::ChannelUp => "KEY_CHUP",
            TizenRemoteKey::ChannelDown => "KEY_CHDOWN",
            TizenRemoteKey::Menu => "KEY_MENU",
            TizenRemoteKey::Source => "KEY_SOURCE",
            TizenRemoteKey::Play => "KEY_PLAY",
            TizenRemoteKey::Pause => "KEY_PAUSE",
            TizenRemoteKey::Stop => "KEY_STOP",
            TizenRemoteKey::Rewind => "KEY_REWIND",
            TizenRemoteKey::FastForward => "KEY_FF",
        }
    }
}

/// `GET /api/v2/applications/{app_id}` — one installed application's status.
///
/// A *point* query, not a list: the bare `/api/v2/applications` form is unverified
/// (see the plan's B2 verdict), so the id comes from the caller — either a preset
/// from [`crate::apps`] or an id read out of the DIAL manifest.
pub const TIZEN_APP_STATUS_PATH: &str = "/api/v2/applications";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_keys_roundtrip_through_their_codes() {
        for key in [
            TizenRemoteKey::Power,
            TizenRemoteKey::Home,
            TizenRemoteKey::VolumeUp,
            TizenRemoteKey::FastForward,
        ] {
            assert_eq!(TizenRemoteKey::from_code(key.as_str()), Some(key));
        }
        assert_eq!(TizenRemoteKey::from_code("KEY_CH_LIST"), None);
    }

    #[test]
    fn raw_codes_are_shape_checked_not_allow_listed() {
        assert!(validate_remote_key_code("KEY_CH_LIST").is_ok());
        assert!(validate_remote_key_code("KEY_0").is_ok());
        assert!(validate_remote_key_code("KEY_HDMI1").is_ok());
        assert!(validate_remote_key_code("KEY_<script>").is_err());
        assert!(validate_remote_key_code("MUTE").is_err());
        assert!(validate_remote_key_code("key_mute").is_err());
        assert!(validate_remote_key_code("").is_err());
    }

    #[test]
    fn launch_actions_have_the_expected_wire_values() {
        assert_eq!(TizenLaunchAction::NativeLaunch.as_str(), "NATIVE_LAUNCH");
        assert_eq!(TizenLaunchAction::DeepLink.as_str(), "DEEP_LINK");
    }
}
