//! Session identifiers and state tracking for screen mirroring.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Identifies a mirroring session.
///
/// As specified in NectarLink `docs/protocol/mirror.md`:
/// - `0` designates the entire phone/device screen.
/// - Any `n > 0` designates an isolated app window.
/// - Missing `session` fields default to `0` for backwards compatibility with older peers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MirrorSessionId(u32);

impl MirrorSessionId {
    /// The primary device screen session (always `0`).
    pub const SCREEN: Self = Self(0);

    /// Creates a new session identifier.
    #[inline]
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    /// Creates an app window session identifier.
    #[inline]
    pub const fn app_window(id: u32) -> Option<Self> {
        if id == 0 {
            None
        } else {
            Some(Self(id))
        }
    }

    /// Returns the raw session number.
    #[inline]
    pub const fn as_u32(self) -> u32 {
        self.0
    }

    /// Returns `true` if this session represents the device screen (session 0).
    #[inline]
    pub const fn is_screen(self) -> bool {
        self.0 == 0
    }

    /// Returns `true` if this session represents an app window (session > 0).
    #[inline]
    pub const fn is_app_window(self) -> bool {
        self.0 > 0
    }
}

impl Default for MirrorSessionId {
    #[inline]
    fn default() -> Self {
        Self::SCREEN
    }
}

impl From<u32> for MirrorSessionId {
    #[inline]
    fn from(val: u32) -> Self {
        Self(val)
    }
}

impl From<MirrorSessionId> for u32 {
    #[inline]
    fn from(session: MirrorSessionId) -> Self {
        session.0
    }
}

impl Serialize for MirrorSessionId {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u32(self.0)
    }
}

impl<'de> Deserialize<'de> for MirrorSessionId {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let val = u32::deserialize(deserializer)?;
        Ok(Self(val))
    }
}

/// Helper function to deserialize an optional session where `None` defaults to `MirrorSessionId::SCREEN`.
pub fn deserialize_default_session<'de, D>(
    deserializer: D,
) -> std::result::Result<MirrorSessionId, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = Option::<u32>::deserialize(deserializer)?;
    Ok(opt
        .map(MirrorSessionId::new)
        .unwrap_or(MirrorSessionId::SCREEN))
}

/// Lifecycle state of a mirroring session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    /// Requested but awaiting peer or user confirmation.
    Pending,
    /// Active and actively streaming video/audio or ready to stream.
    Active,
    /// Paused or waiting for keyframe restart.
    Paused,
    /// Stopped or disconnected.
    Stopped,
}

/// Session descriptor maintaining metadata about an ongoing mirroring session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MirrorSession {
    /// Numbered session identifier.
    pub id: MirrorSessionId,
    /// State of the session.
    pub state: SessionState,
    /// Optional package name if this is an app window session.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_package: Option<String>,
    /// Whether audio is associated with this session.
    pub audio: bool,
    /// Display or virtual window dimensions (width, height), if configured.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dimensions: Option<(u32, u32)>,
}

impl MirrorSession {
    /// Creates a new screen mirroring session (session 0).
    pub fn new_screen(audio: bool) -> Self {
        Self {
            id: MirrorSessionId::SCREEN,
            state: SessionState::Pending,
            app_package: None,
            audio,
            dimensions: None,
        }
    }

    /// Creates a new app window session (session n > 0).
    pub fn new_app_window(session_id: MirrorSessionId, package: impl Into<String>) -> Option<Self> {
        if session_id.is_screen() {
            None
        } else {
            Some(Self {
                id: session_id,
                state: SessionState::Pending,
                app_package: Some(package.into()),
                audio: false,
                dimensions: None,
            })
        }
    }
}
