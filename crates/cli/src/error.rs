//! The CLI's error type.
//!
//! One enum per binary (§4 of the package standard). It aggregates the facade's
//! [`tv_rs::TvError`] via `#[from]`, names the gaps the CLI itself detects
//! (an unknown key spelling, an unsupported operation), and carries the process
//! exit code so `main` can distinguish failure classes for scripts.

use thiserror::Error;

/// Exit code returned for a general failure (matches `std::process::ExitCode::FAILURE`).
pub const EXIT_FAILURE: u8 = 1;

/// Exit code returned when the vendor simply has no protocol for the operation.
///
/// Distinct from [`EXIT_FAILURE`] on purpose: a script can tell "the TV said no"
/// apart from "this vendor cannot do that at all".
pub const EXIT_UNSUPPORTED: u8 = 2;

/// Everything that can go wrong while running a `tv-rs` command.
#[derive(Debug, Error)]
pub enum CliError {
    /// The facade (or a protocol crate it forwarded) returned an error.
    ///
    /// Boxed because [`tv_rs::TvError`] embeds the protocol crates' transport
    /// errors and would otherwise make every `Result` in this binary carry a
    /// large `Err` variant (clippy's `result_large_err`).
    #[error("{0}")]
    Facade(Box<tv_rs::TvError>),

    /// The reconciliation agent could not be built or run.
    #[error("{0}")]
    Agent(String),

    /// The MAC address argument could not be parsed.
    #[error("invalid MAC address '{input}': {source}")]
    MacAddress {
        /// The raw argument the user supplied.
        input: String,
        /// The underlying parse failure.
        #[source]
        source: tv_rs::power::PowerError,
    },

    /// The broadcast address argument was not a valid `IP:PORT` pair.
    #[error(
        "invalid broadcast address '{input}': expected an IP:PORT pair such as 192.168.1.255:9"
    )]
    BroadcastAddress {
        /// The raw argument the user supplied.
        input: String,
    },

    /// A remote-key spelling was not recognised for the chosen vendor.
    #[error("unknown {vendor} remote key '{name}'; valid keys are: {valid}")]
    UnknownKey {
        /// Human-readable vendor name the key was looked up against.
        vendor: &'static str,
        /// The unrecognised spelling the user supplied.
        name: String,
        /// The spellings that would have been accepted.
        valid: String,
    },

    /// The selected vendor's protocol has no equivalent for the operation.
    #[error("{message}")]
    Unsupported {
        /// A diagnostic that names the gap and what was requested.
        message: String,
    },
}

impl From<tv_rs::TvError> for CliError {
    fn from(error: tv_rs::TvError) -> Self {
        Self::Facade(Box::new(error))
    }
}

impl CliError {
    /// The process exit code that distinguishes this failure class.
    ///
    /// [`CliError::Unsupported`] maps to [`EXIT_UNSUPPORTED`]; everything else
    /// maps to [`EXIT_FAILURE`].
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Unsupported { .. } => EXIT_UNSUPPORTED,
            _ => EXIT_FAILURE,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_errors_map_to_the_distinct_exit_code() {
        let error = CliError::Unsupported {
            message: "LG webOS has no remote-keypress API over SSAP".to_string(),
        };

        assert_eq!(error.exit_code(), EXIT_UNSUPPORTED);
        assert_ne!(EXIT_UNSUPPORTED, EXIT_FAILURE);
    }

    #[test]
    fn other_errors_map_to_the_general_failure_code() {
        let broadcast = CliError::BroadcastAddress {
            input: "not-an-address".to_string(),
        };
        assert_eq!(broadcast.exit_code(), EXIT_FAILURE);

        let unknown_key = CliError::UnknownKey {
            vendor: "roku",
            name: "wat".to_string(),
            valid: "home, up".to_string(),
        };
        assert_eq!(unknown_key.exit_code(), EXIT_FAILURE);
    }
}
