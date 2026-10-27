//! USB HID keyboard report structure, keycodes, and mappings for Bluetooth HID emulation.

use tv_rs_core::Key;

/// USB HID keyboard report (8 bytes).
///
/// Standard HID keyboard boot report format:
/// - Byte 0: Modifier keys byte (Ctrl, Shift, Alt, GUI)
/// - Byte 1: Reserved (0x00)
/// - Bytes 2..7: Up to 6 keycodes pressed simultaneously
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HidKeyboardReport {
    /// Modifier mask bitfield (e.g. 0x02 for Left Shift).
    pub modifiers: u8,
    /// Reserved byte (always 0).
    pub reserved: u8,
    /// Array of up to 6 keycodes currently pressed.
    pub keycodes: [u8; 6],
}

impl HidKeyboardReport {
    /// Bitmask for Left Control.
    pub const MOD_LEFT_CTRL: u8 = 0x01;
    /// Bitmask for Left Shift.
    pub const MOD_LEFT_SHIFT: u8 = 0x02;
    /// Bitmask for Left Alt.
    pub const MOD_LEFT_ALT: u8 = 0x04;
    /// Bitmask for Left GUI / Meta / Command.
    pub const MOD_LEFT_GUI: u8 = 0x08;

    /// Creates a empty report with all keys released.
    #[must_use]
    pub fn release_all() -> Self {
        Self::default()
    }

    /// Creates a report for a single keycode press with optional modifiers.
    #[must_use]
    pub fn key_press(keycode: u8, modifiers: u8) -> Self {
        Self {
            modifiers,
            reserved: 0,
            keycodes: [keycode, 0, 0, 0, 0, 0],
        }
    }

    /// Converts the report into an 8-byte slice array for Bluetooth HID transmission.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; 8] {
        [
            self.modifiers,
            self.reserved,
            self.keycodes[0],
            self.keycodes[1],
            self.keycodes[2],
            self.keycodes[3],
            self.keycodes[4],
            self.keycodes[5],
        ]
    }
}

/// USB HID Usage Table keycodes (Keyboard / Keypad Page 0x07).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum HidKeycode {
    /// No key pressed.
    None = 0x00,
    /// 'a' / 'A'
    A = 0x04,
    /// 'b' / 'B'
    B = 0x05,
    /// 'c' / 'C'
    C = 0x06,
    /// 'd' / 'D'
    D = 0x07,
    /// 'e' / 'E'
    E = 0x08,
    /// 'f' / 'F'
    F = 0x09,
    /// 'g' / 'G'
    G = 0x0A,
    /// 'h' / 'H'
    H = 0x0B,
    /// 'i' / 'I'
    I = 0x0C,
    /// 'j' / 'J'
    J = 0x0D,
    /// 'k' / 'K'
    K = 0x0E,
    /// 'l' / 'L'
    L = 0x0F,
    /// 'm' / 'M'
    M = 0x10,
    /// 'n' / 'N'
    N = 0x11,
    /// 'o' / 'O'
    O = 0x12,
    /// 'p' / 'P'
    P = 0x13,
    /// 'q' / 'Q'
    Q = 0x14,
    /// 'r' / 'R'
    R = 0x15,
    /// 's' / 'S'
    S = 0x16,
    /// 't' / 'T'
    T = 0x17,
    /// 'u' / 'U'
    U = 0x18,
    /// 'v' / 'V'
    V = 0x19,
    /// 'w' / 'W'
    W = 0x1A,
    /// 'x' / 'X'
    X = 0x1B,
    /// 'y' / 'Y'
    Y = 0x1C,
    /// 'z' / 'Z'
    Z = 0x1D,
    /// '1'
    Num1 = 0x1E,
    /// '2'
    Num2 = 0x1F,
    /// '3'
    Num3 = 0x20,
    /// '4'
    Num4 = 0x21,
    /// '5'
    Num5 = 0x22,
    /// '6'
    Num6 = 0x23,
    /// '7'
    Num7 = 0x24,
    /// '8'
    Num8 = 0x25,
    /// '9'
    Num9 = 0x26,
    /// '0'
    Num0 = 0x27,
    /// Enter / Return
    Enter = 0x28,
    /// Escape
    Escape = 0x29,
    /// Backspace
    Backspace = 0x2A,
    /// Tab
    Tab = 0x2B,
    /// Space
    Space = 0x2C,
    /// Minus / Underscore
    Minus = 0x2D,
    /// Equal / Plus
    Equal = 0x2E,
    /// Left Bracket
    LeftBracket = 0x2F,
    /// Right Bracket
    RightBracket = 0x30,
    /// Backslash / Pipe
    Backslash = 0x31,
    /// Semicolon / Colon
    Semicolon = 0x33,
    /// Apostrophe / Quote
    Apostrophe = 0x34,
    /// Grave Accent / Tilde
    Grave = 0x35,
    /// Comma / Less Than
    Comma = 0x36,
    /// Dot / Greater Than
    Dot = 0x37,
    /// Slash / Question
    Slash = 0x38,
    /// Right Arrow
    Right = 0x4F,
    /// Left Arrow
    Left = 0x50,
    /// Down Arrow
    Down = 0x51,
    /// Up Arrow
    Up = 0x52,
    /// Menu / Application key
    Menu = 0x65,
    /// Keyboard Power
    Power = 0x66,
    /// Mute (Keyboard / Consumer)
    Mute = 0x7F,
    /// Volume Up (Keyboard / Consumer)
    VolumeUp = 0x80,
    /// Volume Down (Keyboard / Consumer)
    VolumeDown = 0x81,
}

impl HidKeycode {
    /// Returns the raw u8 byte value.
    #[must_use]
    pub fn as_u8(&self) -> u8 {
        *self as u8
    }
}

/// Maps a vendor-neutral [`Key`] from `tv-rs-core` to a USB HID keycode and modifier pair.
#[must_use]
pub fn map_key_to_hid(key: Key) -> Option<(u8, u8)> {
    match key {
        Key::Up => Some((HidKeycode::Up.as_u8(), 0)),
        Key::Down => Some((HidKeycode::Down.as_u8(), 0)),
        Key::Left => Some((HidKeycode::Left.as_u8(), 0)),
        Key::Right => Some((HidKeycode::Right.as_u8(), 0)),
        Key::Select | Key::Enter => Some((HidKeycode::Enter.as_u8(), 0)),
        Key::Back => Some((HidKeycode::Escape.as_u8(), 0)),
        Key::Home => Some((HidKeycode::H.as_u8(), HidKeyboardReport::MOD_LEFT_GUI)),
        Key::Menu => Some((HidKeycode::Menu.as_u8(), 0)),
        Key::Play | Key::Pause => Some((HidKeycode::Space.as_u8(), 0)),
        Key::Stop => Some((HidKeycode::Escape.as_u8(), 0)),
        Key::Backspace => Some((HidKeycode::Backspace.as_u8(), 0)),
        Key::VolumeUp => Some((HidKeycode::VolumeUp.as_u8(), 0)),
        Key::VolumeDown => Some((HidKeycode::VolumeDown.as_u8(), 0)),
        Key::Mute => Some((HidKeycode::Mute.as_u8(), 0)),
        Key::Power | Key::PowerOff => Some((HidKeycode::Power.as_u8(), 0)),
        Key::Digit(d) => match d {
            1..=9 => Some((HidKeycode::Num1.as_u8() + (d - 1), 0)),
            0 => Some((HidKeycode::Num0.as_u8(), 0)),
            _ => None,
        },
        _ => None,
    }
}

/// Maps an ASCII character to a HID keycode and required modifier byte (e.g. Shift).
#[must_use]
pub fn map_ascii_char_to_hid(ch: char) -> Option<(u8, u8)> {
    match ch {
        'a'..='z' => {
            let offset = (ch as u8) - b'a';
            Some((HidKeycode::A.as_u8() + offset, 0))
        }
        'A'..='Z' => {
            let offset = (ch as u8) - b'A';
            Some((
                HidKeycode::A.as_u8() + offset,
                HidKeyboardReport::MOD_LEFT_SHIFT,
            ))
        }
        '1'..='9' => {
            let offset = (ch as u8) - b'1';
            Some((HidKeycode::Num1.as_u8() + offset, 0))
        }
        '0' => Some((HidKeycode::Num0.as_u8(), 0)),
        ' ' => Some((HidKeycode::Space.as_u8(), 0)),
        '\n' | '\r' => Some((HidKeycode::Enter.as_u8(), 0)),
        '\t' => Some((HidKeycode::Tab.as_u8(), 0)),
        '\x08' => Some((HidKeycode::Backspace.as_u8(), 0)),
        '!' => Some((HidKeycode::Num1.as_u8(), HidKeyboardReport::MOD_LEFT_SHIFT)),
        '@' => Some((HidKeycode::Num2.as_u8(), HidKeyboardReport::MOD_LEFT_SHIFT)),
        '#' => Some((HidKeycode::Num3.as_u8(), HidKeyboardReport::MOD_LEFT_SHIFT)),
        '$' => Some((HidKeycode::Num4.as_u8(), HidKeyboardReport::MOD_LEFT_SHIFT)),
        '%' => Some((HidKeycode::Num5.as_u8(), HidKeyboardReport::MOD_LEFT_SHIFT)),
        '^' => Some((HidKeycode::Num6.as_u8(), HidKeyboardReport::MOD_LEFT_SHIFT)),
        '&' => Some((HidKeycode::Num7.as_u8(), HidKeyboardReport::MOD_LEFT_SHIFT)),
        '*' => Some((HidKeycode::Num8.as_u8(), HidKeyboardReport::MOD_LEFT_SHIFT)),
        '(' => Some((HidKeycode::Num9.as_u8(), HidKeyboardReport::MOD_LEFT_SHIFT)),
        ')' => Some((HidKeycode::Num0.as_u8(), HidKeyboardReport::MOD_LEFT_SHIFT)),
        '-' => Some((HidKeycode::Minus.as_u8(), 0)),
        '_' => Some((HidKeycode::Minus.as_u8(), HidKeyboardReport::MOD_LEFT_SHIFT)),
        '=' => Some((HidKeycode::Equal.as_u8(), 0)),
        '+' => Some((HidKeycode::Equal.as_u8(), HidKeyboardReport::MOD_LEFT_SHIFT)),
        '[' => Some((HidKeycode::LeftBracket.as_u8(), 0)),
        '{' => Some((
            HidKeycode::LeftBracket.as_u8(),
            HidKeyboardReport::MOD_LEFT_SHIFT,
        )),
        ']' => Some((HidKeycode::RightBracket.as_u8(), 0)),
        '}' => Some((
            HidKeycode::RightBracket.as_u8(),
            HidKeyboardReport::MOD_LEFT_SHIFT,
        )),
        '\\' => Some((HidKeycode::Backslash.as_u8(), 0)),
        '|' => Some((
            HidKeycode::Backslash.as_u8(),
            HidKeyboardReport::MOD_LEFT_SHIFT,
        )),
        ';' => Some((HidKeycode::Semicolon.as_u8(), 0)),
        ':' => Some((
            HidKeycode::Semicolon.as_u8(),
            HidKeyboardReport::MOD_LEFT_SHIFT,
        )),
        '\'' => Some((HidKeycode::Apostrophe.as_u8(), 0)),
        '"' => Some((
            HidKeycode::Apostrophe.as_u8(),
            HidKeyboardReport::MOD_LEFT_SHIFT,
        )),
        '`' => Some((HidKeycode::Grave.as_u8(), 0)),
        '~' => Some((HidKeycode::Grave.as_u8(), HidKeyboardReport::MOD_LEFT_SHIFT)),
        ',' => Some((HidKeycode::Comma.as_u8(), 0)),
        '<' => Some((HidKeycode::Comma.as_u8(), HidKeyboardReport::MOD_LEFT_SHIFT)),
        '.' => Some((HidKeycode::Dot.as_u8(), 0)),
        '>' => Some((HidKeycode::Dot.as_u8(), HidKeyboardReport::MOD_LEFT_SHIFT)),
        '/' => Some((HidKeycode::Slash.as_u8(), 0)),
        '?' => Some((HidKeycode::Slash.as_u8(), HidKeyboardReport::MOD_LEFT_SHIFT)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hid_report_encoding() {
        let release = HidKeyboardReport::release_all();
        assert_eq!(release.to_bytes(), [0; 8]);

        let press = HidKeyboardReport::key_press(0x04, HidKeyboardReport::MOD_LEFT_SHIFT);
        assert_eq!(
            press.to_bytes(),
            [0x02, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00]
        );
    }

    #[test]
    fn test_key_mapping() {
        assert_eq!(map_key_to_hid(Key::Up), Some((0x52, 0)));
        assert_eq!(map_key_to_hid(Key::Select), Some((0x28, 0)));
        assert_eq!(map_key_to_hid(Key::Back), Some((0x29, 0)));
        assert_eq!(map_key_to_hid(Key::Digit(5)), Some((0x22, 0)));
        assert_eq!(map_key_to_hid(Key::Digit(0)), Some((0x27, 0)));
        assert_eq!(map_key_to_hid(Key::Power), Some((0x66, 0)));
        assert_eq!(map_key_to_hid(Key::PowerOff), Some((0x66, 0)));
    }

    #[test]
    fn test_ascii_char_mapping() {
        assert_eq!(map_ascii_char_to_hid('a'), Some((0x04, 0)));
        assert_eq!(map_ascii_char_to_hid('A'), Some((0x04, 0x02)));
        assert_eq!(map_ascii_char_to_hid('!'), Some((0x1E, 0x02)));
        assert_eq!(map_ascii_char_to_hid(' '), Some((0x2C, 0)));
        assert_eq!(map_ascii_char_to_hid('?'), Some((0x38, 0x02)));
    }
}
