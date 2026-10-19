//! The deferred-capability error vocabulary shared by every protocol crate.

use thiserror::Error;

/// A capability that is deliberately not implemented yet.
///
/// Every `tv-rs` protocol crate reports a blocked reverse-engineering gap with
/// this type instead of `unimplemented!()`, so a published crate can never abort
/// a consumer's process. Each crate re-exposes it as a transparent
/// `Unsupported` variant of its own error enum, which keeps the shared wording
/// and the shared `phase`/`reason` shape in one place.
///
/// # Example
///
/// ```
/// use tv_rs_transport::Unsupported;
///
/// let gap = Unsupported::new(3, "playbook §10 Gap B");
/// assert_eq!(gap.to_string(), "unsupported (phase 3): playbook §10 Gap B");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("unsupported (phase {phase}): {reason}")]
pub struct Unsupported {
    /// Phase in which the gap is expected to be closed.
    pub phase: u8,
    /// Human-readable reason naming the exact playbook gap.
    pub reason: &'static str,
}

impl Unsupported {
    /// Creates a deferred-capability error.
    #[must_use]
    pub const fn new(phase: u8, reason: &'static str) -> Self {
        Self { phase, reason }
    }
}
