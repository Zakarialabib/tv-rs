//! Error types for rule evaluation and engine execution.

use thiserror::Error;

/// Result type alias for rule operations.
pub type Result<T> = std::result::Result<T, RuleError>;

/// Errors that can occur during rule evaluation, schedule parsing, or execution.
#[derive(Debug, Error)]
pub enum RuleError {
    /// Invalid time format string (e.g. expected "HH:MM").
    #[error("invalid time format '{0}', expected 'HH:MM' or 'HH:MM:SS'")]
    InvalidTimeFormat(String),

    /// Referenced scene was not found in registry.
    #[error("scene '{0}' not found in registry")]
    SceneNotFound(String),

    /// Action execution error.
    #[error("action execution failed: {0}")]
    ExecutionFailed(String),

    /// Rule parsing or validation error.
    #[error("rule validation failed: {0}")]
    InvalidRule(String),
}
