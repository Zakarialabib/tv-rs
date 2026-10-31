//! JSON payloads for the Cast protocol namespaces.

use serde::{Deserialize, Serialize};

/// Connection namespace payloads (urn:x-cast:com.google.cast.tp.connection)
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(tag = "type")]
pub enum ConnectionRequest {
    /// Connect message.
    #[serde(rename = "CONNECT")]
    Connect,
    /// Close message.
    #[serde(rename = "CLOSE")]
    Close,
}

/// Heartbeat namespace payloads (urn:x-cast:com.google.cast.tp.heartbeat)
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(tag = "type")]
pub enum HeartbeatRequest {
    /// Ping message.
    #[serde(rename = "PING")]
    Ping,
    /// Pong message.
    #[serde(rename = "PONG")]
    Pong,
}

/// Volume definition for receiver status.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Volume {
    /// Volume level (0.0 to 1.0).
    pub level: Option<f64>,
    /// Muted state.
    pub muted: Option<bool>,
}

/// Application session.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Application {
    /// Application ID.
    pub app_id: String,
    /// Session ID.
    pub session_id: String,
    /// Transport ID for messaging.
    pub transport_id: String,
    /// Display name.
    pub display_name: String,
}

/// Receiver status.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReceiverStatus {
    /// Current applications.
    #[serde(default)]
    pub applications: Vec<Application>,
    /// Current volume.
    pub volume: Option<Volume>,
}

/// Receiver response.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(tag = "type")]
pub enum ReceiverResponse {
    /// Receiver status.
    #[serde(rename = "RECEIVER_STATUS")]
    ReceiverStatus {
        /// Request ID.
        #[serde(rename = "requestId")]
        request_id: i32,
        /// Status details.
        status: ReceiverStatus,
    },
    /// Invalid request error.
    #[serde(rename = "INVALID_REQUEST")]
    InvalidRequest {
        /// Request ID.
        #[serde(rename = "requestId")]
        request_id: i32,
        /// Error reason.
        reason: Option<String>,
    },
}

/// Receiver request.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(tag = "type")]
pub enum ReceiverRequest {
    /// Get status.
    #[serde(rename = "GET_STATUS")]
    GetStatus {
        /// Request ID.
        #[serde(rename = "requestId")]
        request_id: i32,
    },
    /// Launch app.
    #[serde(rename = "LAUNCH")]
    Launch {
        /// Request ID.
        #[serde(rename = "requestId")]
        request_id: i32,
        /// App ID.
        #[serde(rename = "appId")]
        app_id: String,
    },
    /// Stop app.
    #[serde(rename = "STOP")]
    Stop {
        /// Request ID.
        #[serde(rename = "requestId")]
        request_id: i32,
        /// Session ID to stop.
        #[serde(rename = "sessionId")]
        session_id: String,
    },
    /// Set volume.
    #[serde(rename = "SET_VOLUME")]
    SetVolume {
        /// Request ID.
        #[serde(rename = "requestId")]
        request_id: i32,
        /// Volume object.
        volume: Volume,
    },
}

/// Media status.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MediaStatus {
    /// Media session ID.
    pub media_session_id: i32,
    /// Playback state.
    pub player_state: String,
    /// Current time.
    pub current_time: f64,
}

/// Media response.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(tag = "type")]
pub enum MediaResponse {
    /// Media status list.
    #[serde(rename = "MEDIA_STATUS")]
    MediaStatus {
        /// Request ID.
        #[serde(rename = "requestId")]
        request_id: i32,
        /// List of statuses.
        status: Vec<MediaStatus>,
    },
}

/// Media info.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MediaInfo {
    /// Content ID or URL.
    pub content_id: String,
    /// Content type (MIME).
    pub content_type: String,
    /// Stream type.
    pub stream_type: String,
}

/// Media request.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(tag = "type")]
pub enum MediaRequest {
    /// Load media.
    #[serde(rename = "LOAD")]
    Load {
        /// Request ID.
        #[serde(rename = "requestId")]
        request_id: i32,
        /// Media to load.
        media: MediaInfo,
        /// Auto-play flag.
        autoplay: bool,
    },
    /// Play media.
    #[serde(rename = "PLAY")]
    Play {
        /// Request ID.
        #[serde(rename = "requestId")]
        request_id: i32,
        /// Media session ID.
        #[serde(rename = "mediaSessionId")]
        media_session_id: i32,
    },
    /// Pause media.
    #[serde(rename = "PAUSE")]
    Pause {
        /// Request ID.
        #[serde(rename = "requestId")]
        request_id: i32,
        /// Media session ID.
        #[serde(rename = "mediaSessionId")]
        media_session_id: i32,
    },
    /// Stop media.
    #[serde(rename = "STOP")]
    Stop {
        /// Request ID.
        #[serde(rename = "requestId")]
        request_id: i32,
        /// Media session ID.
        #[serde(rename = "mediaSessionId")]
        media_session_id: i32,
    },
    /// Seek media.
    #[serde(rename = "SEEK")]
    Seek {
        /// Request ID.
        #[serde(rename = "requestId")]
        request_id: i32,
        /// Media session ID.
        #[serde(rename = "mediaSessionId")]
        media_session_id: i32,
        /// Current time to seek to.
        #[serde(rename = "currentTime")]
        current_time: f64,
    },
    /// Get status.
    #[serde(rename = "GET_STATUS")]
    GetStatus {
        /// Request ID.
        #[serde(rename = "requestId")]
        request_id: i32,
    },
}
