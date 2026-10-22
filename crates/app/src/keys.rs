//! Remote keys, as a TV webview delivers them.
//!
//! A web app on a television is driven by a D-pad, and the events it receives are
//! ordinary DOM keyboard events — but the *names* are not the same on both platforms
//! and the numeric codes are worse. So this module maps both spellings onto one small
//! vocabulary, and deliberately stops there:
//!
//! - [`Key::from_event_code`] handles the standard `KeyboardEvent.code` / `key`
//!   names. Both Tizen 6+ and webOS deliver these for the D-pad, `Enter` and the
//!   platform back key, and they are what the handoff's wrapper matched on.
//! - [`Key::from_key_code`] handles only the **standard** legacy numbers (the arrows
//!   and `Enter`). Vendor numbers for back and exit differ by platform and by model
//!   year, so they are not guessed here: [`crate::bridge::bridge_js`] takes a
//!   caller-supplied table for them, and a wrong guess never ships as a constant.

/// A remote key, in the vocabulary a TV app cares about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Key {
    /// D-pad up.
    Up,
    /// D-pad down.
    Down,
    /// D-pad left.
    Left,
    /// D-pad right.
    Right,
    /// The OK / enter button.
    Select,
    /// Back to the previous screen.
    Back,
    /// The platform home screen.
    Home,
    /// Exit the app.
    Exit,
    /// Play or resume.
    Play,
    /// Pause.
    Pause,
    /// Stop.
    Stop,
}

impl Key {
    /// Every key, in declaration order.
    pub const ALL: [Key; 11] = [
        Key::Up,
        Key::Down,
        Key::Left,
        Key::Right,
        Key::Select,
        Key::Back,
        Key::Home,
        Key::Exit,
        Key::Play,
        Key::Pause,
        Key::Stop,
    ];

    /// The stable name a payload receives, e.g. `"Select"`.
    ///
    /// This is the string a payload's `key_event(code, value)` is called with, so it
    /// is part of the bridge's contract rather than an internal label.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Up => "Up",
            Self::Down => "Down",
            Self::Left => "Left",
            Self::Right => "Right",
            Self::Select => "Select",
            Self::Back => "Back",
            Self::Home => "Home",
            Self::Exit => "Exit",
            Self::Play => "Play",
            Self::Pause => "Pause",
            Self::Stop => "Stop",
        }
    }

    /// Maps a DOM event name: `KeyboardEvent.code`, or `KeyboardEvent.key` as a
    /// fallback.
    ///
    /// `GoBack` is Tizen's spelling of the back key and `BrowserBack` is webOS's;
    /// both are accepted here so a payload does not have to know which television it
    /// is running on.
    #[must_use]
    pub fn from_event_code(code: &str) -> Option<Self> {
        match code {
            "ArrowUp" | "Up" => Some(Self::Up),
            "ArrowDown" | "Down" => Some(Self::Down),
            "ArrowLeft" | "Left" => Some(Self::Left),
            "ArrowRight" | "Right" => Some(Self::Right),
            "Enter" | "Select" | "OK" => Some(Self::Select),
            "GoBack" | "BrowserBack" | "Backspace" => Some(Self::Back),
            "Home" => Some(Self::Home),
            "Exit" | "Escape" => Some(Self::Exit),
            "MediaPlay" | "MediaPlayPause" => Some(Self::Play),
            "MediaPause" => Some(Self::Pause),
            "MediaStop" => Some(Self::Stop),
            _ => None,
        }
    }

    /// Maps a legacy numeric `KeyboardEvent.keyCode`, for the standard keys only.
    ///
    /// The arrows and `Enter` are the same numbers everywhere; the vendor back and
    /// exit numbers are not, and are supplied per deployment through the bridge's
    /// extra-code table rather than guessed.
    #[must_use]
    pub fn from_key_code(code: u32) -> Option<Self> {
        match code {
            38 => Some(Self::Up),
            40 => Some(Self::Down),
            37 => Some(Self::Left),
            39 => Some(Self::Right),
            13 => Some(Self::Select),
            _ => None,
        }
    }

    /// Resolves a key from either spelling, preferring the event name.
    #[must_use]
    pub fn from_event(code: Option<&str>, key_code: Option<u32>) -> Option<Self> {
        code.and_then(Self::from_event_code)
            .or_else(|| key_code.and_then(Self::from_key_code))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dpad_maps_from_event_names_on_both_platforms() {
        assert_eq!(Key::from_event_code("ArrowUp"), Some(Key::Up));
        assert_eq!(Key::from_event_code("ArrowDown"), Some(Key::Down));
        assert_eq!(Key::from_event_code("ArrowLeft"), Some(Key::Left));
        assert_eq!(Key::from_event_code("ArrowRight"), Some(Key::Right));
        assert_eq!(Key::from_event_code("Enter"), Some(Key::Select));
    }

    #[test]
    fn back_is_accepted_in_both_platform_spellings() {
        // Tizen spells it GoBack, webOS spells it BrowserBack: a payload should not
        // have to know which television it is running on.
        assert_eq!(Key::from_event_code("GoBack"), Some(Key::Back));
        assert_eq!(Key::from_event_code("BrowserBack"), Some(Key::Back));
    }

    #[test]
    fn only_standard_numeric_codes_are_mapped() {
        assert_eq!(Key::from_key_code(38), Some(Key::Up));
        assert_eq!(Key::from_key_code(13), Some(Key::Select));
        // Vendor back/exit numbers are deliberately absent: they differ per platform
        // and per model year, so they come from the deployment, not from here.
        for unmapped in [10009, 461, 10182, 0] {
            assert_eq!(Key::from_key_code(unmapped), None, "{unmapped}");
        }
    }

    #[test]
    fn an_event_name_wins_over_a_numeric_code() {
        assert_eq!(
            Key::from_event(Some("ArrowLeft"), Some(40)),
            Some(Key::Left)
        );
        assert_eq!(Key::from_event(None, Some(40)), Some(Key::Down));
        assert_eq!(Key::from_event(Some("Unmapped"), Some(40)), Some(Key::Down));
        assert_eq!(Key::from_event(None, None), None);
    }

    #[test]
    fn every_key_has_a_stable_name() {
        let names: Vec<&str> = Key::ALL.iter().map(|key| key.as_str()).collect();
        assert_eq!(
            names,
            [
                "Up", "Down", "Left", "Right", "Select", "Back", "Home", "Exit", "Play", "Pause",
                "Stop"
            ]
        );
        // A payload switches on these strings, so they must not collide.
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), names.len());
    }
}
