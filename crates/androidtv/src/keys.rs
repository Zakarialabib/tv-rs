//! Remote key codes and the TvKeys catalog.

use crate::proto::remote::{RemoteDirection, RemoteKeyCode};

/// A remote key code (the `RemoteKeyCode` enum from `remotemessage.proto`).
///
/// The subset below matches `TvKeys.txt` from the reference client; the
/// full enum (0..=304) is available via [`RemoteKeyCode::from_raw`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum Key {
    /// Unknown / no key.
    Unknown = 0,
    /// Home key.
    Home = 3,
    /// Back key.
    Back = 4,
    /// '0'..'9' keys.
    Num0 = 7,
    Num1 = 8,
    Num2 = 9,
    Num3 = 10,
    Num4 = 11,
    Num5 = 12,
    Num6 = 13,
    Num7 = 14,
    Num8 = 15,
    Num9 = 16,
    /// D-pad.
    DpadUp = 19,
    DpadDown = 20,
    DpadLeft = 21,
    DpadRight = 22,
    DpadCenter = 23,
    /// Volume.
    VolumeUp = 24,
    VolumeDown = 25,
    VolumeMute = 165,
    /// Power.
    Power = 26,
    /// Media.
    MediaPlayPause = 85,
    MediaPlay = 126,
    MediaPause = 127,
    MediaNext = 87,
    MediaPrevious = 88,
    MediaStop = 86,
    MediaRewind = 89,
    MediaFastForward = 90,
    /// Channel.
    ChannelUp = 166,
    ChannelDown = 167,
    /// Menu / info.
    Menu = 82,
    Settings = 176,
    /// Red / green / yellow / blue.
    ProgRed = 168,
    ProgGreen = 169,
    ProgYellow = 170,
    ProgBlue = 171,
    /// Search / assist.
    Search = 84,
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
    /// Key down.
    Down,
    /// Key up.
    Up,
    /// Long press start.
    StartLong,
    /// Long press end.
    EndLong,
    /// Short press (tap).
    Short,
}

impl Direction {
    /// The raw `RemoteDirection` value.
    pub fn as_raw(self) -> i32 {
        match self {
            Self::Down => 0,
            Self::Up => 0,
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
