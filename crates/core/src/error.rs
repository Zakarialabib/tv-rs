//! The error taxonomy, and why it is a routing table rather than a description.

use std::fmt;

use crate::capability::Capability;

/// What a caller should do about a failure.
///
/// Every variant answers a question the caller is about to ask: *retry, re-pair,
/// back off, disable a control, or stop?* Adding a variant is a breaking change,
/// because downstream `match` arms are exhaustive over the remedies.
#[derive(Debug, thiserror::Error)]
pub enum TvError {
    /// The device could not be reached, or the connection died. Retry with
    /// backoff; then hand the device to the healer.
    #[error("transport: {0}")]
    Transport(#[from] TransportError),

    /// The device answered, but not in a way this backend understands. Log the
    /// payload, back off, and eventually quarantine.
    #[error("protocol: {0}")]
    Protocol(#[from] ProtocolError),

    /// Credentials are missing, revoked, or refused. Re-provision — never retry
    /// blindly.
    #[error("auth: {0}")]
    Auth(#[from] AuthError),

    /// The device is in a state that forbids the command right now.
    #[error("state: {0}")]
    State(#[from] StateError),

    /// This device cannot do this, and never will. Disable the control.
    #[error("unsupported: {0}")]
    Unsupported(Capability),

    /// A known model/firmware defect. Consult `quirks/` for the documented
    /// fallback instead of guessing.
    #[error("platform quirk: {0}")]
    PlatformQuirk(QuirkId),
}

impl TvError {
    /// The remedy class, for logs, metrics and switch-free dispatch.
    #[must_use]
    pub fn remedy(&self) -> Remedy {
        match self {
            Self::Transport(_) => Remedy::Retry,
            Self::Protocol(_) => Remedy::BackOff,
            Self::Auth(_) => Remedy::Reprovision,
            Self::State(_) => Remedy::Wait,
            Self::Unsupported(_) => Remedy::Disable,
            Self::PlatformQuirk(_) => Remedy::ApplyQuirk,
        }
    }

    /// Shorthand for a protocol error with a message.
    #[must_use]
    pub fn protocol(message: impl Into<String>) -> Self {
        Self::Protocol(ProtocolError::Malformed(message.into()))
    }
}

/// The action a caller should take, one per [`TvError`] variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Remedy {
    /// Try again, with backoff.
    Retry,
    /// The device is answering nonsense; slow down and quarantine on repetition.
    BackOff,
    /// Ask the user (or installer) to pair again.
    Reprovision,
    /// The device is not ready yet; do nothing.
    Wait,
    /// The capability does not exist here; remove the control.
    Disable,
    /// A documented defect with documented handling.
    ApplyQuirk,
}

/// Network-level failure.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TransportError {
    /// Nothing answered at the address.
    #[error("unreachable: {0}")]
    Unreachable(String),
    /// The request was sent and no answer arrived in time.
    #[error("timed out")]
    Timeout,
    /// The TLS handshake failed: the certificate did not match the pin, or the
    /// trust store refused it.
    #[error("tls: {0}")]
    Tls(String),
}

/// The device answered something this backend cannot use.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProtocolError {
    /// The payload could not be parsed.
    #[error("malformed payload: {0}")]
    Malformed(String),
    /// The payload parsed but says something unexpected for this command.
    #[error("unexpected reply: {0}")]
    Unexpected(String),
}

/// Credential or permission failure.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AuthError {
    /// No usable credential yet: the user has to accept the pairing prompt.
    #[error("pairing required")]
    PairingRequired,
    /// A stored credential exists but the device does not accept it any more.
    #[error("token expired or revoked")]
    TokenExpired,
    /// The device forbids the operation behind a setting the user controls.
    ///
    /// Carries the exact remedy so a UI can print it: for example Roku's
    /// *Settings → System → Advanced system settings → Control by mobile apps*.
    #[error("not permitted: {remedy}")]
    NotPermitted {
        /// What the user has to change.
        remedy: String,
    },
}

/// The device is in a state that forbids the command.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StateError {
    /// Not connected, or still coming up.
    #[error("device not ready")]
    NotReady,
    /// Repeatedly failed in the same way; the caller stopped touching it.
    #[error("quarantined")]
    Quarantined,
}

/// Identifier of a known model/firmware defect, resolved against `quirks/`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, thiserror::Error)]
#[error("{0}")]
pub struct QuirkId(String);

impl QuirkId {
    /// Wraps a quirk identifier such as `samsung_8001_disabled`.
    ///
    /// # Panics
    ///
    /// Panics on an empty identifier in debug builds only; release builds accept a
    /// placeholder. An empty quirk id is a programming error, not a runtime input.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        let value = value.into();
        debug_assert!(!value.trim().is_empty(), "quirk id must not be empty");
        Self(value)
    }

    /// The identifier as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every variant routes somewhere. If a variant is added, this function stops
    /// compiling — which is the exhaustiveness test the spec asks for.
    fn route(error: &TvError) -> Remedy {
        match error {
            TvError::Transport(_) => Remedy::Retry,
            TvError::Protocol(_) => Remedy::BackOff,
            TvError::Auth(_) => Remedy::Reprovision,
            TvError::State(_) => Remedy::Wait,
            TvError::Unsupported(_) => Remedy::Disable,
            TvError::PlatformQuirk(_) => Remedy::ApplyQuirk,
        }
    }

    #[test]
    fn remedy_matches_the_exhaustive_match() {
        let cases = [
            TvError::Transport(TransportError::Timeout),
            TvError::protocol("bad xml"),
            TvError::Auth(AuthError::PairingRequired),
            TvError::State(StateError::NotReady),
            TvError::Unsupported(Capability::Key),
            TvError::PlatformQuirk(QuirkId::new("samsung_8001_disabled")),
        ];
        for error in cases {
            assert_eq!(error.remedy(), route(&error), "{error}");
        }
    }

    #[test]
    fn errors_display_the_remedy_for_the_user() {
        let error = TvError::Auth(AuthError::NotPermitted {
            remedy: "enable Settings → System → Control by mobile apps".to_string(),
        });
        let text = error.to_string();
        assert!(text.contains("not permitted"), "{text}");
        assert!(text.contains("Control by mobile apps"), "{text}");
    }

    #[test]
    fn unsupported_names_the_capability() {
        assert_eq!(
            TvError::Unsupported(Capability::InputSelect).to_string(),
            "unsupported: input_select"
        );
    }

    #[test]
    fn errors_convert_from_their_parts() {
        let error: TvError = TransportError::Unreachable("192.168.1.5:8060".into()).into();
        assert_eq!(error.remedy(), Remedy::Retry);
    }
}
