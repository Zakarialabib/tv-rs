//! Automation actions executed when rules trigger.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Severity or level for notifications.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationLevel {
    /// Informational notification.
    Info,
    /// Warning alert.
    Warning,
    /// Critical emergency alert.
    Critical,
}

/// Action to be taken when a rule triggers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action_type", rename_all = "snake_case")]
pub enum Action {
    /// Display a message on a target TV/signage screen.
    ShowMessage {
        /// Target TV or display ID (e.g. "entrance_tv").
        target: String,
        /// Text message content to display.
        message: String,
        /// Optional duration in seconds for overlay display.
        #[serde(skip_serializing_if = "Option::is_none")]
        duration_secs: Option<u64>,
    },

    /// Send a notification to a recipient or system role (e.g. manager).
    Notify {
        /// Recipient or channel (e.g. "manager", "operations").
        recipient: String,
        /// Notification message text.
        message: String,
        /// Notification alert level.
        level: NotificationLevel,
    },

    /// Power control action for target display.
    PowerControl {
        /// Target TV ID.
        target: String,
        /// `true` to power on, `false` to power off.
        power_on: bool,
    },

    /// Switch input source on target TV.
    SwitchSource {
        /// Target TV ID.
        target: String,
        /// Input source name (e.g. "HDMI1", "DigitalSignage").
        source: String,
    },

    /// Trigger activation of a named scene.
    ActivateScene {
        /// Scene ID to activate.
        scene_id: String,
    },

    /// Send an HTTP webhook payload.
    Webhook {
        /// Webhook target URL.
        url: String,
        /// HTTP method (e.g. "POST").
        method: String,
        /// Optional body payload.
        #[serde(skip_serializing_if = "Option::is_none")]
        body: Option<String>,
    },

    /// Custom extensibility action.
    Custom {
        /// Action subtype or domain.
        name: String,
        /// Target entity.
        target: String,
        /// Generic JSON payload.
        payload: serde_json::Value,
    },
}

impl Action {
    /// Returns a target key string for conflict resolution (e.g. `"entrance_tv:message"`).
    /// Actions targeting the same domain on the same resource compete for execution.
    pub fn target_key(&self) -> Option<String> {
        match self {
            Self::ShowMessage { target, .. } => Some(format!("{target}:message")),
            Self::PowerControl { target, .. } => Some(format!("{target}:power")),
            Self::SwitchSource { target, .. } => Some(format!("{target}:source")),
            Self::Custom { target, name, .. } => Some(format!("{target}:custom:{name}")),
            _ => None,
        }
    }
}

/// Async handler trait for executing actions.
#[async_trait]
pub trait ActionExecutor: Send + Sync {
    /// Executes a single action.
    async fn execute(&self, action: &Action) -> crate::Result<()>;
}
