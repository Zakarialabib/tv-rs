//! User input event types forwarded to mirrored displays or app windows.

use crate::session::{deserialize_default_session, MirrorSessionId};
use serde::{Deserialize, Serialize};

/// Touch interaction action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TouchAction {
    /// Finger down.
    Down,
    /// Finger move / dragging.
    Move,
    /// Finger up.
    Up,
}

/// Single-finger touch input event.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TouchInput {
    /// Action: down, move, up.
    pub action: TouchAction,
    /// Horizontal coordinate as a fraction of display width (0.0 to 1.0).
    pub x: f32,
    /// Vertical coordinate as a fraction of display height (0.0 to 1.0).
    pub y: f32,
}

/// Mouse wheel or touchpad scroll event.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ScrollInput {
    /// Horizontal coordinate fraction (0.0 to 1.0).
    pub x: f32,
    /// Vertical coordinate fraction (0.0 to 1.0).
    pub y: f32,
    /// Horizontal scroll delta notches (positive: right).
    pub dx: f32,
    /// Vertical scroll delta notches (positive: down).
    pub dy: f32,
}

/// Standard key action strings supported by NectarLink mirror.input.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyAction {
    /// Back navigation button.
    Back,
    /// Home button.
    Home,
    /// Recent apps overview.
    Recents,
    /// Notification shade.
    Notifications,
    /// Screen lock toggle.
    Lock,
    /// Enter key.
    Enter,
    /// Backspace key.
    Backspace,
    /// Delete key.
    Delete,
    /// Left arrow.
    Left,
    /// Right arrow.
    Right,
    /// Up arrow.
    Up,
    /// Down arrow.
    Down,
    /// Tab key.
    Tab,
    /// Copy selected text.
    Copy,
    /// Cut selected text.
    Cut,
    /// Select all text.
    SelectAll,
    /// Action / send button on active IME node.
    Send,
    /// Typing started without video stream.
    KeyboardOn,
    /// Typing ended without video stream.
    KeyboardOff,
    /// Custom / vendor key string.
    #[serde(untagged)]
    Other(String),
}

/// Gamepad controller input state forwarded to display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GamepadInput {
    /// 16-bit XInput-compatible button bitmask.
    pub buttons: u16,
    /// Left thumbstick X axis (-32768..=32767).
    pub lx: i16,
    /// Left thumbstick Y axis (-32768..=32767).
    pub ly: i16,
    /// Right thumbstick X axis (-32768..=32767).
    pub rx: i16,
    /// Right thumbstick Y axis (-32768..=32767).
    pub ry: i16,
    /// Left trigger axis (0..=255).
    pub lt: u8,
    /// Right trigger axis (0..=255).
    pub rt: u8,
}

/// Mirror input event message (`mirror.input`).
///
/// Sent from PC/controller to phone or display device.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum InputEvent {
    /// Touch input.
    Touch(TouchInput),
    /// Scroll input.
    Scroll(ScrollInput),
    /// Navigation / function key.
    Key {
        /// Standard key action.
        key: KeyAction,
    },
    /// Text input (up to 4 KiB typed characters).
    Text {
        /// Typed text string.
        text: String,
    },
    /// Gamepad controller state.
    Gamepad(GamepadInput),
}

/// Full input message envelope including optional session routing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MirrorInputMessage {
    /// Target session (session 0 for screen, session n for app window).
    #[serde(default, deserialize_with = "deserialize_default_session")]
    pub session: MirrorSessionId,

    /// Concrete input event payload.
    #[serde(flatten)]
    pub event: InputEvent,
}
