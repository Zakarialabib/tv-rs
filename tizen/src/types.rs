//! Constants and enums for the Samsung Tizen protocol.

use serde::{Deserialize, Serialize};

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

/// Prefix for DIAL app-launch requests (`POST {prefix}{app_id}`).
pub const TIZEN_DIAL_LAUNCH_PREFIX: &str = "/ws/apps/";

/// Application identifier this crate advertises to the TV.
pub const TIZEN_APP_ID: &str = "tv-rs";

/// Value of the `Cmd` field used for remote key clicks.
pub const TIZEN_CMD_CLICK: &str = "Click";

/// Value of the `TypeOfRemote` field used for remote key clicks.
pub const TIZEN_REMOTE_TYPE: &str = "SendRemoteKey";

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
