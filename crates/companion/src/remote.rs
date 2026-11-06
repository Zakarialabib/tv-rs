//! Remote key definitions for companion.

use tv_rs_core::Key;

/// Remote key command representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteKey {
    /// Navigate up.
    Up,
    /// Navigate down.
    Down,
    /// Navigate left.
    Left,
    /// Navigate right.
    Right,
    /// Select / Enter.
    Select,
    /// Back / Return.
    Back,
    /// Home button.
    Home,
    /// Volume up.
    VolumeUp,
    /// Volume down.
    VolumeDown,
    /// Mute toggle.
    Mute,
    /// Play/Pause.
    PlayPause,
}

impl From<RemoteKey> for Key {
    fn from(k: RemoteKey) -> Self {
        match k {
            RemoteKey::Up => Key::Up,
            RemoteKey::Down => Key::Down,
            RemoteKey::Left => Key::Left,
            RemoteKey::Right => Key::Right,
            RemoteKey::Select => Key::Select,
            RemoteKey::Back => Key::Back,
            RemoteKey::Home => Key::Home,
            RemoteKey::VolumeUp => Key::VolumeUp,
            RemoteKey::VolumeDown => Key::VolumeDown,
            RemoteKey::Mute => Key::Mute,
            RemoteKey::PlayPause => Key::Play,
        }
    }
}
