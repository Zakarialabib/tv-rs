//! The error a consumer sees, and the one place vendor errors become it.
//!
//! Every protocol crate keeps its own error enum — `TizenError` knows about SSAP
//! tokens, `RokuError` knows about ECP's permission gate, and collapsing them into
//! one enum at the source would throw that away. But a consumer driving three
//! vendors must not have to match three error types and guess at a fourth, so the
//! facade exposes exactly one: [`tv_rs_core::TvError`], whose variants are a
//! *routing table* — each one already answers "what should the caller do now?".
//!
//! `tv_rs::TvError` is that type, re-exported. There is no facade-specific error
//! enum: a second enum is a second mapping to keep in sync, and the bug this module
//! exists to prevent (see below) is exactly a mapping that fell behind.

pub use tv_rs_core::TvError;

/// Convenience alias for results returned by the facade.
pub type Result<T> = std::result::Result<T, TvError>;
