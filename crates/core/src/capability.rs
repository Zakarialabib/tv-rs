//! Capability vocabulary and the erased device handle.

use std::fmt;
use std::time::Duration;

use async_trait::async_trait;

use crate::command::CommandId;
use crate::descriptor::{DeviceDescriptor, DeviceId, PowerState};
use crate::error::TvError;
use crate::telemetry::TelemetryProvider;

/// One thing a device can be asked to do.
///
/// Capabilities are coarse on purpose: a UI greys out a control, and a fleet
/// report counts devices, neither of which needs per-key granularity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum Capability {
    /// Read the power state.
    PowerState,
    /// Wake the device from standby (Wake-on-LAN or vendor equivalent).
    Wake,
    /// Send a remote key.
    Key,
    /// Launch an application.
    AppLaunch,
    /// Ask which application is in front.
    Foreground,
    /// Read and change the active input.
    InputSelect,
    /// Read and change volume / mute.
    Volume,
    /// Read and change panel brightness.
    Brightness,
    /// Reboot the device.
    Reboot,
    /// Read diagnostic telemetry such as temperature and signal quality.
    Telemetry,
    /// Lock the panel's own keys so nobody can walk up and change the picture.
    PanelLock,
    /// Read and set the orientation the panel displays content in.
    Orientation,
    /// Read whether the panel's input actually has a signal, and its health.
    NoSignalStatus,
    /// Read how long the panel (or its lamp) has been running.
    #[cfg(feature = "supporting")]
    LampOrPanelHours,
    /// Grab a still frame of what the panel is showing.
    #[cfg(feature = "supporting")]
    ScreenCapture,
    /// Type a string into a field on the device (a search box, a login).
    ///
    /// Its own capability rather than part of [`Capability::Key`]: on some platforms
    /// a string is a single service call (an IME, a base64 payload) rather than a run
    /// of key presses, so "has a keypad" and "can type a string" are independent
    /// facts and the UI needs them separately.
    TextInput,
    /// List the applications installed on the device.
    ///
    /// Its own capability rather than part of [`Capability::AppLaunch`]: a platform
    /// can start an application it cannot enumerate (Tizen's remote channel is
    /// exactly that), so "can I launch" and "can I list what is installed" are
    /// independent facts, and the fleet grid renders a blank column on the wrong
    /// one. Declared last so the `CapabilitySet` bit layout of every capability
    /// before it is unchanged.
    AppList,
}

impl Capability {
    /// A stable snake_case label for logs and diagnostics.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PowerState => "power_state",
            Self::Wake => "wake",
            Self::Key => "key",
            Self::AppLaunch => "app_launch",
            Self::Foreground => "foreground",
            Self::InputSelect => "input_select",
            Self::Volume => "volume",
            Self::Brightness => "brightness",
            Self::Reboot => "reboot",
            Self::Telemetry => "telemetry",
            Self::PanelLock => "panel_lock",
            Self::Orientation => "orientation",
            Self::NoSignalStatus => "no_signal_status",
            #[cfg(feature = "supporting")]
            Self::LampOrPanelHours => "lamp_or_panel_hours",
            #[cfg(feature = "supporting")]
            Self::ScreenCapture => "screen_capture",
            Self::TextInput => "text_input",
            Self::AppList => "app_list",
        }
    }
}

/// The set of capabilities a device advertises.
///
/// Deliberately not a bitflags crate: a hand-written set keeps the dependency list
/// of the core honest. It widened from `u16` to `u32` when text entry arrived — the
/// sixteenth capability filled the old width — and the widening is additive: every
/// capability keeps its index, so a set written by an older build reads the same.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CapabilitySet(u32);

impl CapabilitySet {
    /// An empty set.
    #[must_use]
    pub fn empty() -> Self {
        Self(0)
    }

    /// A set containing exactly `capability`.
    #[must_use]
    pub fn of(capability: Capability) -> Self {
        Self(1 << capability_index(capability))
    }

    /// Adds a capability.
    pub fn insert(&mut self, capability: Capability) {
        self.0 |= 1 << capability_index(capability);
    }

    /// Builder form of [`CapabilitySet::insert`].
    #[must_use]
    pub fn with(mut self, capability: Capability) -> Self {
        self.insert(capability);
        self
    }

    /// Whether the set contains `capability`.
    #[must_use]
    pub fn contains(&self, capability: Capability) -> bool {
        self.0 & (1 << capability_index(capability)) != 0
    }

    /// Whether the set is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0 == 0
    }

    /// Iterates the capabilities present, in declaration order.
    pub fn iter(&self) -> impl Iterator<Item = Capability> + '_ {
        ALL_CAPABILITIES
            .iter()
            .copied()
            .filter(|capability| self.contains(*capability))
    }
}

const ALL_CAPABILITIES: &[Capability] = &[
    Capability::PowerState,
    Capability::Wake,
    Capability::Key,
    Capability::AppLaunch,
    Capability::Foreground,
    Capability::InputSelect,
    Capability::Volume,
    Capability::Brightness,
    Capability::Reboot,
    Capability::Telemetry,
    Capability::PanelLock,
    Capability::Orientation,
    Capability::NoSignalStatus,
    #[cfg(feature = "supporting")]
    Capability::LampOrPanelHours,
    #[cfg(feature = "supporting")]
    Capability::ScreenCapture,
    Capability::AppList,
    Capability::TextInput,
];

const fn capability_index(capability: Capability) -> u16 {
    match capability {
        Capability::PowerState => 0,
        Capability::Wake => 1,
        Capability::Key => 2,
        Capability::AppLaunch => 3,
        Capability::Foreground => 4,
        Capability::InputSelect => 5,
        Capability::Volume => 6,
        Capability::Brightness => 7,
        Capability::Reboot => 8,
        Capability::Telemetry => 9,
        Capability::PanelLock => 10,
        Capability::Orientation => 11,
        Capability::NoSignalStatus => 12,
        #[cfg(feature = "supporting")]
        Capability::LampOrPanelHours => 13,
        #[cfg(feature = "supporting")]
        Capability::ScreenCapture => 14,
        Capability::AppList => 15,
        Capability::TextInput => 16,
    }
}

/// A remote key, in vendor-neutral vocabulary.
///
/// The set is the intersection of what automation actually needs: navigation to
/// escape a modal, playback control, volume, and power. A vendor key that has no
/// equivalent here is reachable through the backend's own client.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Key {
    /// Up.
    Up,
    /// Down.
    Down,
    /// Left.
    Left,
    /// Right.
    Right,
    /// Confirm / OK.
    Select,
    /// Back / return.
    Back,
    /// Home screen.
    Home,
    /// Menu / settings overlay.
    Menu,
    /// Play or resume.
    Play,
    /// Pause.
    Pause,
    /// Stop.
    Stop,
    /// Rewind / scan backwards.
    Rewind,
    /// Fast-forward.
    FastForward,
    /// Previous track / chapter.
    Previous,
    /// Next track / chapter.
    Next,
    /// Playback information.
    Info,
    /// Channel up.
    ChannelUp,
    /// Channel down.
    ChannelDown,
    /// Volume up.
    VolumeUp,
    /// Volume down.
    VolumeDown,
    /// Volume mute toggle.
    Mute,
    /// Power toggle.
    Power,
    /// Power off.
    PowerOff,
    /// Open the search screen.
    Search,
    /// Submit text input.
    Enter,
    /// Delete the previous character.
    Backspace,
    /// Numeric key, 0–9.
    Digit(u8),
}

impl Key {
    /// A stable snake_case label for logs.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Up => "up",
            Self::Down => "down",
            Self::Left => "left",
            Self::Right => "right",
            Self::Select => "select",
            Self::Back => "back",
            Self::Home => "home",
            Self::Menu => "menu",
            Self::Play => "play",
            Self::Pause => "pause",
            Self::Stop => "stop",
            Self::Rewind => "rewind",
            Self::FastForward => "fast_forward",
            Self::Previous => "previous",
            Self::Next => "next",
            Self::Info => "info",
            Self::ChannelUp => "channel_up",
            Self::ChannelDown => "channel_down",
            Self::VolumeUp => "volume_up",
            Self::VolumeDown => "volume_down",
            Self::Mute => "mute",
            Self::Power => "power",
            Self::PowerOff => "power_off",
            Self::Search => "search",
            Self::Enter => "enter",
            Self::Backspace => "backspace",
            Self::Digit(_) => "digit",
        }
    }

    /// Builds a numeric key.
    ///
    /// # Errors
    ///
    /// Returns [`TvError::Unsupported`] for digits outside 0–9: a remote has ten,
    /// and silently clamping would send a different key than the caller asked for.
    pub fn digit(value: u8) -> Result<Self, TvError> {
        if value > 9 {
            return Err(TvError::Unsupported(Capability::Key));
        }
        Ok(Self::Digit(value))
    }
}

/// An input a device can be switched to.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Input {
    /// HDMI port, 1-based as printed on the panel.
    Hdmi(u8),
    /// The tuner.
    Tv,
    /// USB port, 1-based.
    Usb(u8),
    /// A vendor-specific input this crate does not model.
    Other(String),
}

impl Input {
    /// A stable label for logs and UI.
    #[must_use]
    pub fn as_str(&self) -> String {
        match self {
            Self::Hdmi(port) => format!("hdmi{port}"),
            Self::Tv => "tv".to_string(),
            Self::Usb(port) => format!("usb{port}"),
            Self::Other(name) => name.clone(),
        }
    }
}

impl fmt::Display for Input {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.as_str())
    }
}

/// A reference to an application: id plus, when known, its display name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AppRef {
    /// Vendor-specific application id.
    pub id: String,
    /// Human-readable name, when the caller knows it.
    pub name: Option<String>,
}

impl AppRef {
    /// Builds a reference from an id alone.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: None,
        }
    }

    /// Attaches a display name.
    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }
}

impl fmt::Display for AppRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name.as_deref().unwrap_or(&self.id))
    }
}

/// Power and wake.
#[async_trait]
pub trait PowerControl: Send + Sync {
    /// Reads the current power state. Unobservable devices return
    /// [`PowerState::Unknown`] rather than an error.
    async fn power_state(&self) -> Result<PowerState, TvError>;

    /// Wakes the device. Devices that cannot be woken return
    /// [`TvError::Unsupported`] — never a silent success.
    async fn wake(&self) -> Result<(), TvError>;

    /// Puts the device into standby (or off, where standby does not exist).
    async fn sleep(&self) -> Result<(), TvError>;
}

/// Remote keys.
#[async_trait]
pub trait KeyInput: Send + Sync {
    /// Sends one key press.
    ///
    /// Two deliveries of the same `cmd` must have the effect of one — see
    /// [`CommandId`].
    async fn key_press(&self, key: Key, cmd: CommandId) -> Result<(), TvError>;
}

/// Application launch and foreground detection.
#[async_trait]
pub trait AppLaunch: Send + Sync {
    /// Launches an application.
    ///
    /// Success means *the device accepted the request*, not *the app is in front*:
    /// call [`AppLaunch::foreground`] to verify.
    async fn launch(&self, app: AppRef, cmd: CommandId) -> Result<(), TvError>;

    /// Reports the foreground application, when the platform can say.
    async fn foreground(&self) -> Result<Option<AppRef>, TvError>;

    /// Lists installed applications, when the platform can say.
    ///
    /// An empty `Vec` is a real answer ("this TV exposes none"); `Unsupported`
    /// means the platform cannot list at all. The default is `Unsupported`, so an
    /// implementation that cannot list says so instead of inventing a catalogue.
    async fn installed_apps(&self) -> Result<Vec<AppRef>, TvError> {
        // Names [`Capability::AppList`], not [`Capability::AppLaunch`]: a platform
        // that cannot enumerate may well be able to launch, and the caller needs to
        // know which of the two it was refused.
        Err(TvError::Unsupported(Capability::AppList))
    }
}

/// Typing a string into a field on the device: a search box, a login form.
///
/// Distinct from [`KeyInput`]: a string can be one service call (an IME, a base64
/// payload) rather than a run of key presses, so a device may be able to type
/// without this trait and unable to type with it.
#[async_trait]
pub trait TextInput: Send + Sync {
    /// Types `text` into the focused field.
    ///
    /// An empty string is a no-op. A character the platform cannot prove it can
    /// send is refused rather than skipped — a search box that quietly drops a
    /// character looks like it worked, and the caller needs to know it did not.
    async fn type_text(&self, _text: &str, _cmd: CommandId) -> Result<(), TvError> {
        Err(TvError::Unsupported(Capability::TextInput))
    }
}

/// Input selection.
#[async_trait]
pub trait InputSelect: Send + Sync {
    /// Reports the active input, when the platform can say.
    async fn active_input(&self) -> Result<Option<Input>, TvError>;

    /// Switches to an input.
    async fn select_input(&self, input: Input, cmd: CommandId) -> Result<(), TvError>;
}

/// Volume and mute.
#[async_trait]
pub trait VolumeControl: Send + Sync {
    /// Reads the volume, when the platform can say.
    async fn volume(&self) -> Result<Option<u8>, TvError>;

    /// Sets the volume, 0–100.
    ///
    /// Platforms that only expose relative keys map "up one step" onto this call
    /// and report [`crate::Confidence::Inferred`]; they do not claim the level was
    /// set.
    async fn set_volume(&self, level: u8, cmd: CommandId) -> Result<(), TvError>;

    /// Mutes or unmutes.
    async fn set_mute(&self, muted: bool, cmd: CommandId) -> Result<(), TvError>;

    /// Reads the mute state, when the platform can say.
    ///
    /// `Ok(None)` means "cannot tell" — the UI shows an unknown mute dot rather
    /// than the blind toggle it was forced into while this did not exist.
    async fn mute(&self) -> Result<Option<bool>, TvError>;
}

/// Panel brightness.
#[async_trait]
pub trait BrightnessControl: Send + Sync {
    /// Reads brightness as a percentage, when the platform can report it.
    async fn brightness(&self) -> Result<Option<u8>, TvError>;

    /// Sets brightness, 0-100.
    ///
    /// A backend that only accepts relative steps must report the value as
    /// [`crate::Confidence::Inferred`] rather than claiming the requested level
    /// was reached.
    async fn set_brightness(&self, level: u8, cmd: CommandId) -> Result<(), TvError>;
}

/// A device that can be rebooted without removing power.
#[async_trait]
pub trait Rebootable: Send + Sync {
    /// Requests a reboot.
    ///
    /// Success means the request was accepted, not that the device has finished
    /// restarting; callers must wait for it to answer again before sending another
    /// command.
    async fn reboot(&self, cmd: CommandId) -> Result<(), TvError>;
}

/// A control-plane round-trip measurement.
///
/// This deliberately measures the same channel commands use, not ICMP: a TV that
/// answers `ping` but not its control port is still unreachable for this library.
#[async_trait]
pub trait Probe: Send + Sync {
    /// Performs one cheap control-channel round trip and returns its latency.
    async fn ping(&self) -> Result<Duration, TvError>;
}

/// The orientation a panel displays its content in.
///
/// Commercial panels are mounted portrait as often as landscape, and a wall built
/// from rotated panels needs the content rotated with it. The flipped variants exist
/// because a panel can be physically mounted upside down in a ceiling rig.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Orientation {
    /// Upright landscape (the default).
    Landscape,
    /// Landscape rotated 180° — a panel hanging upside down.
    LandscapeFlipped,
    /// Portrait, rotated 90°.
    Portrait,
    /// Portrait rotated 180°.
    PortraitFlipped,
}

impl Orientation {
    /// A stable snake_case label for logs and diagnostics.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Landscape => "landscape",
            Self::LandscapeFlipped => "landscape_flipped",
            Self::Portrait => "portrait",
            Self::PortraitFlipped => "portrait_flipped",
        }
    }

    /// Whether the content is taller than it is wide.
    #[must_use]
    pub fn is_portrait(&self) -> bool {
        matches!(self, Self::Portrait | Self::PortraitFlipped)
    }
}

/// What a panel reports about its input signal and its own health.
///
/// This is the answer to the question a playout monitor actually asks: *is anything
/// arriving, and is the panel well?* A panel with no signal is not broken — the
/// source is — and a report that conflated the two would send someone to the wrong
/// screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanelHealth {
    /// Whether the active input carries a signal the panel can lock onto.
    pub signal_present: bool,
    /// Panel temperature in Celsius, when the panel reports one.
    pub temperature_celsius: Option<i16>,
    /// Whether the panel reports a lamp (backlight) fault.
    pub lamp_error: bool,
}

/// How long a panel has been running.
///
/// Asset and thermal planning needs the number, not a guess: a panel at 40 000 hours
/// is a panel to plan a replacement for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanelHours {
    /// Total hours the panel has been powered on.
    pub panel_hours: u32,
    /// Total lamp hours, when the panel reports them separately from panel hours.
    pub lamp_hours: Option<u32>,
}

/// The encoding of a captured frame.
#[cfg(feature = "supporting")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameFormat {
    /// JPEG.
    Jpeg,
    /// PNG.
    Png,
}

/// A still frame captured from a panel.
#[cfg(feature = "supporting")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenFrame {
    /// How [`ScreenFrame::bytes`] are encoded.
    pub format: FrameFormat,
    /// The encoded image.
    pub bytes: Vec<u8>,
}

/// Panel key lock — the "nobody walks up and changes the picture" surface.
#[async_trait]
pub trait PanelLockControl: Send + Sync {
    /// Whether the panel's own keys are locked, when the panel can say.
    async fn panel_lock(&self) -> Result<Option<bool>, TvError>;

    /// Locks or unlocks the panel's own keys.
    async fn set_panel_lock(&self, locked: bool, cmd: CommandId) -> Result<(), TvError>;
}

/// The orientation a panel displays its content in.
#[async_trait]
pub trait OrientationControl: Send + Sync {
    /// The current content orientation, when the panel can report it.
    async fn orientation(&self) -> Result<Option<Orientation>, TvError>;

    /// Sets the content orientation.
    async fn set_orientation(
        &self,
        orientation: Orientation,
        cmd: CommandId,
    ) -> Result<(), TvError>;
}

/// Input-signal state and panel health.
#[async_trait]
pub trait PanelHealthProvider: Send + Sync {
    /// Reads the panel's signal state and health, when the protocol exposes it.
    async fn panel_health(&self) -> Result<Option<PanelHealth>, TvError>;
}

/// Panel or lamp running hours.
#[cfg(feature = "supporting")]
#[async_trait]
pub trait PanelHoursProvider: Send + Sync {
    /// Reads how long the panel has been running, when the protocol exposes it.
    async fn panel_hours(&self) -> Result<Option<PanelHours>, TvError>;
}

/// Still-frame capture from a panel.
#[cfg(feature = "supporting")]
#[async_trait]
pub trait ScreenCapturer: Send + Sync {
    /// Captures one frame, when the protocol exposes it.
    async fn capture(&self) -> Result<Option<ScreenFrame>, TvError>;
}

/// An erased television.
///
/// The fleet reason this trait exists: `Vec<Arc<dyn DeviceHandle>>` compiles, and a
/// caller learns what a device can do *before* it sends anything.
pub trait DeviceHandle: Send + Sync {
    /// Stable identifier.
    ///
    /// Returned by value because a handle may be a thin wrapper around a protocol
    /// client that never stored one; the cost is a small clone, not a lifetime.
    fn id(&self) -> DeviceId;

    /// Everything known before a command is sent.
    ///
    /// A snapshot, not a borrow: backends that learn more after pairing can return
    /// richer descriptors without the caller holding a lock or a lifetime.
    fn descriptor(&self) -> DeviceDescriptor;

    /// The capabilities this device advertises.
    fn capabilities(&self) -> CapabilitySet;

    /// Power control, or `None` when the device cannot do it.
    fn as_power(&self) -> Option<&dyn PowerControl>;

    /// Key input, or `None`.
    fn as_key(&self) -> Option<&dyn KeyInput>;

    /// Application control, or `None`.
    fn as_app(&self) -> Option<&dyn AppLaunch>;

    /// Text entry, or `None`.
    fn as_text(&self) -> Option<&dyn TextInput>;

    /// Input selection, or `None`.
    fn as_input(&self) -> Option<&dyn InputSelect>;

    /// Volume, or `None`.
    fn as_volume(&self) -> Option<&dyn VolumeControl>;

    /// Brightness, or `None`.
    ///
    /// Defaulted to `None` so existing backends compile unchanged; a backend that
    /// advertises [`Capability::Brightness`] must override it.
    fn as_brightness(&self) -> Option<&dyn BrightnessControl> {
        None
    }

    /// Reboot control, or `None`.
    ///
    /// Defaulted to `None` for the same reason as [`DeviceHandle::as_brightness`].
    fn as_reboot(&self) -> Option<&dyn Rebootable> {
        None
    }

    /// Panel key lock, or `None`.
    ///
    /// Defaulted to `None` so existing backends compile unchanged; a backend that
    /// advertises [`Capability::PanelLock`] must override it.
    fn as_panel_lock(&self) -> Option<&dyn PanelLockControl> {
        None
    }

    /// Content orientation, or `None`.
    fn as_orientation(&self) -> Option<&dyn OrientationControl> {
        None
    }

    /// Input-signal state and panel health, or `None`.
    fn as_panel_health(&self) -> Option<&dyn PanelHealthProvider> {
        None
    }

    /// Panel or lamp hours, or `None`.
    #[cfg(feature = "supporting")]
    fn as_panel_hours(&self) -> Option<&dyn PanelHoursProvider> {
        None
    }

    /// Still-frame capture, or `None`.
    #[cfg(feature = "supporting")]
    fn as_screen_capturer(&self) -> Option<&dyn ScreenCapturer> {
        None
    }

    /// Control-plane latency measurement, or `None`.
    fn as_probe(&self) -> Option<&dyn Probe> {
        None
    }

    /// Diagnostic telemetry, or `None`.
    ///
    /// Defaulted to `None`; a backend that advertises [`Capability::Telemetry`]
    /// must override it.
    fn as_telemetry(&self) -> Option<&dyn TelemetryProvider> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_sets_contain_what_was_added() {
        let set = CapabilitySet::empty()
            .with(Capability::PowerState)
            .with(Capability::Key);
        assert!(set.contains(Capability::PowerState));
        assert!(set.contains(Capability::Key));
        assert!(!set.contains(Capability::Volume));
        assert_eq!(
            set.iter().collect::<Vec<_>>(),
            vec![Capability::PowerState, Capability::Key]
        );
    }

    #[test]
    fn capability_flags_do_not_collide() {
        let mut seen = std::collections::HashSet::new();
        for capability in ALL_CAPABILITIES.iter().copied() {
            assert!(
                // The shift width must follow the set, not be pinned here: the set
                // widened from u16 to u32 when the sixteenth capability arrived, and
                // a hardcoded width is exactly how that would have been missed.
                seen.insert(1u32 << capability_index(capability)),
                "duplicate bit for {capability}"
            );
            assert!(CapabilitySet::of(capability).contains(capability));
        }
        assert_eq!(seen.len(), ALL_CAPABILITIES.len());
    }

    #[test]
    fn digits_are_checked() {
        assert!(Key::digit(7).is_ok());
        assert!(Key::digit(10).is_err());
    }

    #[test]
    fn inputs_render_stably() {
        assert_eq!(Input::Hdmi(2).as_str(), "hdmi2");
        assert_eq!(Input::Usb(1).to_string(), "usb1");
        assert_eq!(Input::Tv.to_string(), "tv");
        assert_eq!(Input::Other("AV".into()).to_string(), "AV");
    }

    #[test]
    fn app_refs_prefer_the_name() {
        let app = AppRef::new("3201512006786").with_name("Prime Video");
        assert_eq!(app.to_string(), "Prime Video");
        assert_eq!(AppRef::new("guess").to_string(), "guess");
    }
}
