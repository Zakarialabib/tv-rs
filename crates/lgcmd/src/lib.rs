//! LG commercial ASCII serial protocol (ka/mc) over TCP 9761 or RS-232.

#![forbid(unsafe_code)]

/// Client implementation for the LG commercial ASCII serial protocol.
pub mod client;
/// Error types for LG commercial ASCII serial protocol operations.
pub mod error;
/// Command constants for the LG commercial ASCII serial protocol.
pub mod protocol;
