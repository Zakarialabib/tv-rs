//! Imperative commands: the low-latency remote verbs a lease holder sends.
//!
//! An [`ImperativeCommand`] is "do this now" as opposed to desired-state's
//! "this should be true". It maps one-to-one onto two capabilities —
//! [`Capability::Key`] and [`Capability::TextInput`] — and onto the
//! [`DeviceHandle`] accessors for each, so the agent's relay branch can dispatch
//! a command over a warm socket without touching the reconciler.
//!
//! The type deliberately models nothing else: a lease holder who wants an app
//! launch, a volume write or a reboot is asking the fleet to mutate state, and
//! that goes through desired state and the normal ladder. Keys and text are the
//! verbs that must feel like a remote: sub-second, idempotent-by-caller, and
//! never re-planned.

use crate::capability::{Capability, DeviceHandle, Key};
use crate::command::CommandId;
use crate::error::TvError;

/// A single remote verb.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(tag = "kind", content = "payload", rename_all = "snake_case")
)]
pub enum ImperativeCommand {
    /// Press a remote key.
    ///
    /// Maps onto [`Capability::Key`] and the erased
    /// [`DeviceHandle::as_key`] accessor.
    Key {
        /// The key to press.
        key: Key,
    },
    /// Type a string into the focused field.
    ///
    /// Maps onto [`Capability::TextInput`] and the erased
    /// [`DeviceHandle::as_text`] accessor.
    Text {
        /// The string to type.
        payload: String,
    },
}

impl ImperativeCommand {
    /// The capability this command asks for, so the caller can know whether a
    /// device can take it before any wire is touched.
    #[must_use]
    pub fn capability(&self) -> Capability {
        match self {
            Self::Key { .. } => Capability::Key,
            Self::Text { .. } => Capability::TextInput,
        }
    }

    /// A stable `snake_case` label for logs and audit entries.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Key { .. } => "key",
            Self::Text { .. } => "text",
        }
    }

    /// Dispatches the command through the erased handle, returning the refusal
    /// the vocabulary has already made typed: a device without the capability
    /// gets [`TvError::Unsupported`], not a runtime surprise.
    ///
    /// The caller holds the `CommandId`, because idempotency is the sender's
    /// contract (the facade registry treats two deliveries of one id as one).
    ///
    /// # Errors
    ///
    /// Returns [`TvError::Unsupported`] when the device does not advertise the
    /// capability — the same fact `as_key()` / `as_text()` returning `None`
    /// would — and whatever the backend returns on send failure otherwise.
    pub async fn dispatch(
        &self,
        handle: &dyn DeviceHandle,
        cmd: CommandId,
    ) -> Result<(), TvError> {
        match self {
            Self::Key { key } => match handle.as_key() {
                Some(input) => input.key_press(*key, cmd).await,
                None => Err(TvError::Unsupported(Capability::Key)),
            },
            Self::Text { payload } => match handle.as_text() {
                Some(input) => input.type_text(payload, cmd).await,
                None => Err(TvError::Unsupported(Capability::TextInput)),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_and_capabilities_map_one_to_one() {
        let key = ImperativeCommand::Key { key: Key::Home };
        assert_eq!(key.kind(), "key");
        assert_eq!(key.capability(), Capability::Key);

        let text = ImperativeCommand::Text {
            payload: "hello".to_string(),
        };
        assert_eq!(text.kind(), "text");
        assert_eq!(text.capability(), Capability::TextInput);
    }

    #[test]
    #[cfg(feature = "serde")]
    fn serialization_round_trips() {
        for command in [
            ImperativeCommand::Key { key: Key::Digit(5) },
            ImperativeCommand::Text {
                payload: "hello".to_string(),
            },
        ] {
            let json = serde_json::to_string(&command).unwrap();
            let parsed: ImperativeCommand = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, command);
        }
    }
}
