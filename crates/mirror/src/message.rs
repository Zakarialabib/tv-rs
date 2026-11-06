//! Control messages for screen mirroring matching NectarLink `docs/protocol/mirror.md`.

use crate::error::{MirrorError, Result};
use crate::session::{deserialize_default_session, MirrorSessionId};
use serde::{Deserialize, Serialize};

/// Start a mirroring session (`mirror.start`).
///
/// Sent from controller/receiver to phone or source device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MirrorStart {
    /// Maximum size of the longer side in pixels (e.g. 1920).
    pub max_size: u32,

    /// Maximum frames per second (e.g. 60).
    pub fps: u32,

    /// Target bitrate in bits per second (e.g. 8_000_000).
    pub bitrate: u32,

    /// Whether audio playback capture should be streamed alongside video. Defaults to false.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub audio: bool,

    /// Numbered session: 0 is full screen; >0 is an app window.
    /// Missing session deserializes to 0 for backwards compatibility.
    #[serde(default, deserialize_with = "deserialize_default_session")]
    pub session: MirrorSessionId,

    /// App package name (e.g. "com.android.chrome") for virtual display app window sessions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,

    /// Keep device screen from sleeping during screen mirror (session 0 only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stay_awake: Option<bool>,

    /// Turn off physical panel while streaming (session 0 only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screen_off: Option<bool>,

    /// Target player identifier or "Play on phone" watch mode (session 0 only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub watch: Option<String>,

    /// Sound only mode without video (session 0 with audio, never with app/watch/stay_awake/screen_off).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sound_only: Option<bool>,
}

impl MirrorStart {
    /// Constructs a builder for `mirror.start`.
    pub fn builder(max_size: u32, fps: u32, bitrate: u32) -> MirrorStartBuilder {
        MirrorStartBuilder::new(max_size, fps, bitrate)
    }

    /// Validates protocol consistency according to NectarLink §2 & §6.
    pub fn validate(&self) -> Result<()> {
        if let Some(true) = self.sound_only {
            if !self.session.is_screen() {
                return Err(MirrorError::InvalidConfiguration(
                    "sound_only requires session 0".into(),
                ));
            }
            if !self.audio {
                return Err(MirrorError::InvalidConfiguration(
                    "sound_only requires audio = true".into(),
                ));
            }
            if self.app.is_some() {
                return Err(MirrorError::InvalidConfiguration(
                    "sound_only cannot be used with app".into(),
                ));
            }
            if self.stay_awake.is_some() || self.screen_off.is_some() {
                return Err(MirrorError::InvalidConfiguration(
                    "sound_only cannot be used with stay_awake or screen_off".into(),
                ));
            }
            if self.watch.is_some() {
                return Err(MirrorError::InvalidConfiguration(
                    "sound_only cannot be used with watch".into(),
                ));
            }
        }

        if let Some(ref _app) = self.app {
            if self.session.is_screen() {
                return Err(MirrorError::InvalidConfiguration(
                    "app parameter requires non-zero session".into(),
                ));
            }
            if self.watch.is_some() {
                return Err(MirrorError::InvalidConfiguration(
                    "app cannot be used with watch".into(),
                ));
            }
        } else if self.session.is_app_window() {
            return Err(MirrorError::InvalidConfiguration(
                "non-zero session requires app package parameter".into(),
            ));
        }

        Ok(())
    }
}

/// Builder for `MirrorStart` requests.
#[derive(Debug, Clone)]
pub struct MirrorStartBuilder {
    start: MirrorStart,
}

impl MirrorStartBuilder {
    /// Creates a new builder with primary video parameters.
    pub fn new(max_size: u32, fps: u32, bitrate: u32) -> Self {
        Self {
            start: MirrorStart {
                max_size,
                fps,
                bitrate,
                audio: false,
                session: MirrorSessionId::SCREEN,
                app: None,
                stay_awake: None,
                screen_off: None,
                watch: None,
                sound_only: None,
            },
        }
    }

    /// Sets whether audio capture should accompany the video stream.
    pub fn with_audio(mut self, audio: bool) -> Self {
        self.start.audio = audio;
        self
    }

    /// Targets a specific session ID (0 for screen, >0 for app window).
    pub fn with_session(mut self, session: MirrorSessionId) -> Self {
        self.start.session = session;
        self
    }

    /// Configures an app window launch on a virtual display.
    pub fn with_app(mut self, session: MirrorSessionId, app_package: impl Into<String>) -> Self {
        self.start.session = session;
        self.start.app = Some(app_package.into());
        self
    }

    /// Keeps the display panel awake while mirrored.
    pub fn with_stay_awake(mut self, stay_awake: bool) -> Self {
        self.start.stay_awake = Some(stay_awake);
        self
    }

    /// Turns off physical screen panel while mirroring.
    pub fn with_screen_off(mut self, screen_off: bool) -> Self {
        self.start.screen_off = Some(screen_off);
        self
    }

    /// Configures watch mode.
    pub fn with_watch(mut self, watch: impl Into<String>) -> Self {
        self.start.watch = Some(watch.into());
        self
    }

    /// Configures sound-only mode (audio only, no video).
    pub fn with_sound_only(mut self) -> Self {
        self.start.session = MirrorSessionId::SCREEN;
        self.start.audio = true;
        self.start.sound_only = Some(true);
        self.start.app = None;
        self.start.stay_awake = None;
        self.start.screen_off = None;
        self.start.watch = None;
        self
    }

    /// Builds and returns the validated `MirrorStart` command.
    pub fn build(self) -> MirrorStart {
        self.start
    }
}

/// Stop a mirroring session (`mirror.stop`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MirrorStop {
    /// Numbered session to stop. Missing session defaults to 0.
    #[serde(default, deserialize_with = "deserialize_default_session")]
    pub session: MirrorSessionId,
}

impl MirrorStop {
    /// Creates a stop message for the screen session (0).
    pub fn screen() -> Self {
        Self {
            session: MirrorSessionId::SCREEN,
        }
    }

    /// Creates a stop message for an app window session.
    pub fn app_window(session: MirrorSessionId) -> Self {
        Self { session }
    }
}

/// Update display power flags during screen mirror (`mirror.power`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MirrorPower {
    /// Update stay awake setting.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stay_awake: Option<bool>,

    /// Update screen off setting.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screen_off: Option<bool>,
}

/// Request an immediate keyframe from the encoder (`mirror.keyframe`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MirrorKeyframe {
    /// Target session. Missing session defaults to 0.
    #[serde(default, deserialize_with = "deserialize_default_session")]
    pub session: MirrorSessionId,
}

impl MirrorKeyframe {
    /// Requests a keyframe for the screen session.
    pub fn screen() -> Self {
        Self {
            session: MirrorSessionId::SCREEN,
        }
    }

    /// Requests a keyframe for an app window session.
    pub fn app_window(session: MirrorSessionId) -> Self {
        Self { session }
    }
}

/// Resize virtual display for an app window (`mirror.resize`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MirrorResize {
    /// App window session ID (must be non-zero).
    pub session: MirrorSessionId,

    /// Width in pixels.
    pub width: u32,

    /// Height in pixels.
    pub height: u32,
}

impl MirrorResize {
    /// Creates a resize request, validating that session != 0.
    pub fn new(session: MirrorSessionId, width: u32, height: u32) -> Result<Self> {
        if session.is_screen() {
            return Err(MirrorError::NonZeroSessionRequired);
        }
        Ok(Self {
            session,
            width,
            height,
        })
    }
}

/// Toggle audio playback sending (`mirror.sound`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MirrorSound {
    /// Whether sound sending is enabled or disabled.
    pub on: bool,
}

/// Request launchable apps list (`mirror.apps` query).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MirrorAppsQuery {}

/// Application descriptor for `mirror.apps` response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppInfo {
    /// Package name (e.g. "org.mozilla.firefox").
    pub pkg: String,

    /// User-visible app label (e.g. "Firefox").
    pub label: String,

    /// Optional raw PNG icon bytes (at most 8 KiB).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<Vec<u8>>,
}

/// Response payload containing launchable apps (`mirror.apps` response).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AppListPayload {
    /// List of launchable apps (up to 500 apps).
    pub apps: Vec<AppInfo>,
}

/// Response wrapper for `mirror.apps`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MirrorAppsResponse {
    /// Response body containing apps.
    pub b: AppListPayload,
}
