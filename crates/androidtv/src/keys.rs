//! Remote key codes and the TvKeys catalog.

use crate::proto::remote::RemoteDirection;

/// A remote key code (the `RemoteKeyCode` enum from `remotemessage.proto`).
///
/// The variants below are the subset from `TvKeys.txt` in the reference
/// client; arbitrary raw codes are available via [`Key::from_raw`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum Key {
    /// Unknown / no key (`KEYCODE_UNKNOWN`).
    Unknown = 0,
    /// Home key (`KEYCODE_HOME`).
    Home = 3,
    /// Back key (`KEYCODE_BACK`).
    Back = 4,
    /// '0' key (`KEYCODE_0`).
    Num0 = 7,
    /// '1' key (`KEYCODE_1`).
    Num1 = 8,
    /// '2' key (`KEYCODE_2`).
    Num2 = 9,
    /// '3' key (`KEYCODE_3`).
    Num3 = 10,
    /// '4' key (`KEYCODE_4`).
    Num4 = 11,
    /// '5' key (`KEYCODE_5`).
    Num5 = 12,
    /// '6' key (`KEYCODE_6`).
    Num6 = 13,
    /// '7' key (`KEYCODE_7`).
    Num7 = 14,
    /// '8' key (`KEYCODE_8`).
    Num8 = 15,
    /// '9' key (`KEYCODE_9`).
    Num9 = 16,
    /// D-pad up (`KEYCODE_DPAD_UP`).
    DpadUp = 19,
    /// D-pad down (`KEYCODE_DPAD_DOWN`).
    DpadDown = 20,
    /// D-pad left (`KEYCODE_DPAD_LEFT`).
    DpadLeft = 21,
    /// D-pad right (`KEYCODE_DPAD_RIGHT`).
    DpadRight = 22,
    /// D-pad center (`KEYCODE_DPAD_CENTER`).
    DpadCenter = 23,
    /// Volume up (`KEYCODE_VOLUME_UP`).
    VolumeUp = 24,
    /// Volume down (`KEYCODE_VOLUME_DOWN`).
    VolumeDown = 25,
    /// Power key (`KEYCODE_POWER`).
    Power = 26,
    /// Menu key (`KEYCODE_MENU`).
    Menu = 82,
    /// Search key (`KEYCODE_SEARCH`).
    Search = 84,
    /// Play/pause toggle (`KEYCODE_MEDIA_PLAY_PAUSE`).
    MediaPlayPause = 85,
    /// Stop (`KEYCODE_MEDIA_STOP`).
    MediaStop = 86,
    /// Next track (`KEYCODE_MEDIA_NEXT`).
    MediaNext = 87,
    /// Previous track (`KEYCODE_MEDIA_PREVIOUS`).
    MediaPrevious = 88,
    /// Rewind (`KEYCODE_MEDIA_REWIND`).
    MediaRewind = 89,
    /// Fast forward (`KEYCODE_MEDIA_FAST_FORWARD`).
    MediaFastForward = 90,
    /// Play (`KEYCODE_MEDIA_PLAY`).
    MediaPlay = 126,
    /// Pause (`KEYCODE_MEDIA_PAUSE`).
    MediaPause = 127,
    /// Volume mute (`KEYCODE_VOLUME_MUTE`).
    VolumeMute = 165,
    /// Channel up (`KEYCODE_CHANNEL_UP`).
    ChannelUp = 166,
    /// Channel down (`KEYCODE_CHANNEL_DOWN`).
    ChannelDown = 167,
    /// Red color key (`KEYCODE_PROG_RED`).
    ProgRed = 168,
    /// Green color key (`KEYCODE_PROG_GREEN`).
    ProgGreen = 169,
    /// Yellow color key (`KEYCODE_PROG_YELLOW`).
    ProgYellow = 170,
    /// Blue color key (`KEYCODE_PROG_BLUE`).
    ProgBlue = 171,
    /// Settings key (`KEYCODE_SETTINGS`).
    Settings = 176,
    /// Voice assist key (`KEYCODE_ASSIST`).
    Assist = 219,
}

impl Key {
    /// The raw `RemoteKeyCode` value for this key.
    pub fn as_raw(self) -> i32 {
        self as i32
    }

    /// Build a `Key` from a raw `RemoteKeyCode` value.
    ///
    /// Returns `None` for values outside the `TvKeys.txt` catalog.
    pub fn from_raw(raw: i32) -> Option<Self> {
        Some(match raw {
            0 => Self::Unknown,
            3 => Self::Home,
            4 => Self::Back,
            7 => Self::Num0,
            8 => Self::Num1,
            9 => Self::Num2,
            10 => Self::Num3,
            11 => Self::Num4,
            12 => Self::Num5,
            13 => Self::Num6,
            14 => Self::Num7,
            15 => Self::Num8,
            16 => Self::Num9,
            19 => Self::DpadUp,
            20 => Self::DpadDown,
            21 => Self::DpadLeft,
            22 => Self::DpadRight,
            23 => Self::DpadCenter,
            24 => Self::VolumeUp,
            25 => Self::VolumeDown,
            26 => Self::Power,
            82 => Self::Menu,
            84 => Self::Search,
            85 => Self::MediaPlayPause,
            86 => Self::MediaStop,
            87 => Self::MediaNext,
            88 => Self::MediaPrevious,
            89 => Self::MediaRewind,
            90 => Self::MediaFastForward,
            126 => Self::MediaPlay,
            127 => Self::MediaPause,
            165 => Self::VolumeMute,
            166 => Self::ChannelUp,
            167 => Self::ChannelDown,
            168 => Self::ProgRed,
            169 => Self::ProgGreen,
            170 => Self::ProgYellow,
            171 => Self::ProgBlue,
            176 => Self::Settings,
            219 => Self::Assist,
            _ => return None,
        })
    }
}

/// A key press direction (the `RemoteDirection` enum).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Key down (`UNKNOWN_DIRECTION`, the default).
    Down,
    /// Key up.
    Up,
    /// Long press start (`START_LONG`).
    StartLong,
    /// Long press end (`END_LONG`).
    EndLong,
    /// Short press / tap (`SHORT`).
    Short,
}

impl Direction {
    /// The raw `RemoteDirection` value.
    pub fn as_raw(self) -> i32 {
        match self {
            Self::Down | Self::Up => RemoteDirection::UnknownDirection as i32,
            Self::StartLong => RemoteDirection::StartLong as i32,
            Self::EndLong => RemoteDirection::EndLong as i32,
            Self::Short => RemoteDirection::Short as i32,
        }
    }
}

/// Convert a `Key` + `Direction` into a `RemoteKeyInject` protobuf.
pub fn key_inject(key: Key, direction: Direction) -> crate::proto::remote::RemoteKeyInject {
    crate::proto::remote::RemoteKeyInject {
        key_code: key.as_raw(),
        direction: direction.as_raw(),
    }
}
