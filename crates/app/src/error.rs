//! What can go wrong while packaging.
//!
//! Three failures, and each one is something the caller can act on: fix a descriptor
//! field, fix a path, or install the webOS CLI. Nothing here is a panic and nothing
//! is a bare string — a caller that cannot tell "your app id is wrong" from "the disk
//! is full" has to read a log to find out, which is the thing this workspace refuses
//! to make people do.

use std::path::PathBuf;

/// A packaging failure.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// A descriptor field a television would have rejected at install time.
    #[error("{field} is not usable: {reason}")]
    InvalidDescriptor {
        /// The field that failed.
        field: &'static str,
        /// Why it failed.
        reason: &'static str,
    },

    /// A filesystem failure while reading the payload or writing the artifact.
    #[error("io error at {path}: {source}")]
    Io {
        /// The path involved.
        path: PathBuf,
        /// The underlying error.
        #[source]
        source: std::io::Error,
    },

    /// Assembling a `.ipk` needs the webOS CLI, which is not part of this crate.
    ///
    /// A webOS `.ipk` is an `ar` archive holding `control.tar.gz` and `data.tar.gz`
    /// with a platform-specific control file. Reimplementing that here would be a
    /// second, unaudited packaging path; the crate emits the app directory the CLI
    /// wants and names the command instead.
    #[error("assembling a .ipk needs the webOS CLI (`ares-package`); run: {command}")]
    RequiresWebosCli {
        /// The exact command to run, with the app directory already in it.
        command: String,
    },
}

impl AppError {
    /// Wraps an [`std::io::Error`] with the path it happened on.
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}
