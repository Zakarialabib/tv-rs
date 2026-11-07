//! Command identity and the idempotency contract.

use std::fmt;

use uuid::Uuid;

use crate::capability::{Input, Key};

/// An identifier a caller attaches to a mutating command.
///
/// # The contract
///
/// Backends **do not** deduplicate. The window that makes deduplication meaningful
/// is a deployment property — roughly 30 seconds for an app launch, a few seconds
/// for a keypress — so it belongs to the layer that owns retries, not to the
/// protocol crate.
///
/// What the backend guarantees instead is stated once, here: *two deliveries of the
/// same `CommandId` must have the effect of one.* A backend that cannot honour that
/// (because the protocol has no request identity) says so in its capability docs
/// rather than pretending.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CommandId(Uuid);

impl CommandId {
    /// Generates a fresh identifier.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Wraps an existing UUID, e.g. one that arrived from a queue.
    #[must_use]
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// The underlying UUID.
    #[must_use]
    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for CommandId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for CommandId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A command sent to a television through [`Driver::send`](crate::driver::Driver::send).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Press a remote key.
    Key(Key),
    /// Type text into an active input field.
    Text(String),
    /// Set power state.
    Power(bool),
    /// Launch an application by identifier.
    LaunchApp(String),
    /// Switch active input.
    Input(Input),
    /// Set absolute volume level (0-100).
    Volume(u8),
    /// Set audio mute.
    Mute(bool),
    /// Raw or vendor-specific command string.
    Custom {
        /// Verb or category name.
        name: String,
        /// Command payload.
        payload: String,
    },
}

/// Acknowledgement returned after sending a command to a driver.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommandAck {
    /// Associated command id, if provided.
    pub id: Option<CommandId>,
    /// Optional status or response message from the device.
    pub message: Option<String>,
}

impl CommandAck {
    /// Builds an acknowledgement without an attached command id.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds an acknowledgement tied to a specific [`CommandId`].
    #[must_use]
    pub fn with_id(id: CommandId) -> Self {
        Self {
            id: Some(id),
            message: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CommandId;

    #[test]
    fn generated_ids_are_distinct() {
        assert_ne!(CommandId::new(), CommandId::new());
    }

    #[test]
    fn ids_are_reproducible_from_a_uuid() {
        let id = CommandId::new();
        assert_eq!(CommandId::from_uuid(id.as_uuid()), id);
        assert_eq!(id.to_string(), id.as_uuid().to_string());
    }

    #[test]
    fn ids_hash_and_compare_by_value() {
        let id = CommandId::new();
        let copy = id;
        let mut set = std::collections::HashSet::new();
        set.insert(id);
        assert!(set.contains(&copy));
    }

    #[test]
    fn command_ack_constructors() {
        use super::{Command, CommandAck, Key};

        let ack = CommandAck::new();
        assert_eq!(ack.id, None);
        assert_eq!(ack.message, None);

        let id = CommandId::new();
        let ack_with_id = CommandAck::with_id(id);
        assert_eq!(ack_with_id.id, Some(id));

        let cmd = Command::Key(Key::Power);
        assert_eq!(cmd, Command::Key(Key::Power));
    }
}
