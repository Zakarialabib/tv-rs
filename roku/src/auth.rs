//! Authentication marker for Roku ECP.
//!
//! ECP is genuinely unauthenticated. The Roku HTTP server on port 8060 accepts
//! requests with no credentials *as long as they originate on the local subnet*.
//! A user can still restrict access from the TV-side "Control by mobile apps"
//! setting, which is a device policy rather than a credential handshake.
//!
//! This trait exists so the `tv-rs` facade can perform a uniform, no-op auth
//! step across vendors (Samsung Tizen and LG webOS do require pairing, Roku does
//! not). It intentionally carries no secret and no token.

use crate::client::RokuClient;

/// Marker trait describing the (absent) authentication requirements of ECP.
pub trait RokuAuthProvider {
    /// Reports whether an authentication step is required before issuing ECP
    /// commands.
    ///
    /// Always `false` for Roku: ECP requires no credentials from the local
    /// subnet. Access can still be disabled by the TV's "Control by mobile apps"
    /// setting, in which case commands simply fail at the transport level.
    fn is_authentication_required(&self) -> bool {
        false
    }
}

impl RokuAuthProvider for RokuClient {}
