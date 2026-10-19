//! Unified error type for the `tv-rs` facade.
//!
//! Every protocol crate keeps its own error enum; the facade aggregates them so
//! a caller that drives several vendors does not have to match five error types.

use thiserror::Error;

/// Errors surfaced by the facade.
///
/// Each variant wraps the originating protocol crate's error, so the specific
/// cause is never lost.
#[derive(Debug, Error)]
pub enum TvError {
    /// Discovery failed: SSDP socket, descriptor HTTP fetch, or XML parsing.
    #[error("discovery error: {0}")]
    Discovery(#[from] tv_rs_discovery::DiscoveryError),

    /// Wake-on-LAN broadcast or power-state probing failed.
    #[error("power error: {0}")]
    Power(#[from] tv_rs_power::PowerError),

    /// Samsung Tizen control failed.
    #[error("tizen error: {0}")]
    Tizen(#[from] tv_rs_tizen::TizenError),

    /// LG webOS control failed.
    #[error("webos error: {0}")]
    Webos(#[from] tv_rs_webos::WebosError),

    /// Roku ECP control failed.
    #[error("roku error: {0}")]
    Roku(#[from] tv_rs_roku::RokuError),

    /// The device was discovered but matched no known vendor signature, so no
    /// control client could be constructed for it.
    #[error("no control client available for {ip}: {reason}")]
    UnsupportedVendor {
        /// IP address of the device that could not be classified.
        ip: String,
        /// Why classification failed.
        reason: String,
    },

    /// The vendor's protocol has no equivalent for the requested operation.
    #[error("{vendor} does not support {operation} over its local control protocol")]
    UnsupportedOperation {
        /// Human-readable vendor name.
        vendor: &'static str,
        /// Human-readable operation name.
        operation: &'static str,
    },
}

/// Convenience alias for results returned by the facade.
pub type Result<T> = std::result::Result<T, TvError>;
