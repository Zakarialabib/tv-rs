//! Constants and the SSAP command URI enumeration for LG webOS.

use serde::{Deserialize, Serialize};

/// Legacy, unencrypted LG webOS SSAP WebSocket port.
pub const WEBOS_WS_PORT: u16 = 3000;

/// Modern, TLS-wrapped LG webOS SSAP WebSocket port.
pub const WEBOS_WSS_PORT: u16 = 3001;

/// Prefix shared by every SSAP command URI.
pub const SSAP_URI_PREFIX: &str = "ssap://";

/// Pairing type that makes the TV display an on-screen "Allow?" prompt.
pub const WEBOS_PAIRING_TYPE_PROMPT: &str = "PROMPT";

/// Default SSAP manifest permissions requested during registration.
pub const WEBOS_DEFAULT_PERMISSIONS: &[&str] = &[
    "CONTROL_POWER",
    "READ_INSTALLED_APPS",
    "CONTROL_INPUT_TV",
    "READ_TV_CHANNEL",
    "WRITE_CURRENT_CHANNEL",
    "CONTROL_AUDIO",
    "READ_AUDIO",
    "CONTROL_INPUT_MEDIA_PLAYBACK",
];

/// Default file name used when persisting the SSAP `client-key`.
pub const WEBOS_CLIENT_KEY_FILE: &str = "webos-client-key.json";

/// The SSAP command URIs known to this Phase-1 client.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SsapUri {
    /// Launch an installed application by id.
    SystemLauncherLaunch,
    /// List the applications installed on the TV.
    SystemLauncherListApps,
    /// Report the application currently in the foreground.
    SystemLauncherForegroundApp,
    /// Set the output volume (0-100).
    AudioSetVolume,
    /// Read the current output volume.
    AudioGetVolume,
    /// Enable or disable mute.
    AudioSetMute,
    /// Read the current mute state.
    AudioGetMute,
    /// Turn the TV off.
    SystemTurnOff,
    /// Turn the TV screen off while keeping the main CPU awake.
    TvTurnOffScreen,
    /// Pause media playback.
    MediaPause,
    /// Resume media playback.
    MediaPlay,
    /// Stop media playback.
    MediaStop,
    /// Ask the TV for the pointer/mouse input socket.
    ///
    /// The response carries a `socketPath`; it is a complete WebSocket URL for a
    /// second, per-session connection that accepts plain-text pointer frames.
    NetworkInputGetPointerSocket,
    /// Open a URL in the TV's browser.
    ///
    /// This is the only content channel webOS offers without deploying an app:
    /// `system.launcher/open` with a `target` URL puts the built-in browser on
    /// screen. Everything a signage operator needs to show starts here.
    SystemLauncherOpen,
}

impl SsapUri {
    /// Returns the exact SSAP command URI string for this variant.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SystemLauncherLaunch => "ssap://system.launcher/launch",
            Self::SystemLauncherListApps => "ssap://com.webos.applicationManager/listApps",
            Self::SystemLauncherForegroundApp => {
                "ssap://com.webos.applicationManager/getForegroundAppInfo"
            }
            Self::AudioSetVolume => "ssap://audio/setVolume",
            Self::AudioGetVolume => "ssap://audio/getVolume",
            Self::AudioSetMute => "ssap://audio/setMute",
            Self::AudioGetMute => "ssap://audio/getMute",
            Self::SystemTurnOff => "ssap://system/turnOff",
            Self::TvTurnOffScreen => "ssap://com.webos.service.tv.power/turnOffScreen",
            Self::MediaPause => "ssap://media.controls/pause",
            Self::MediaPlay => "ssap://media.controls/play",
            Self::MediaStop => "ssap://media.controls/stop",
            Self::NetworkInputGetPointerSocket => {
                "ssap://com.webos.service.networkinput/getPointerInputSocket"
            }
            Self::SystemLauncherOpen => "ssap://system.launcher/open",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SsapUri;

    #[test]
    fn ssap_uri_as_str_matches_documented_uris() {
        assert_eq!(
            SsapUri::SystemLauncherLaunch.as_str(),
            "ssap://system.launcher/launch"
        );
        assert_eq!(SsapUri::AudioSetVolume.as_str(), "ssap://audio/setVolume");
        assert_eq!(SsapUri::SystemTurnOff.as_str(), "ssap://system/turnOff");
    }
}
