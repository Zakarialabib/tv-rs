//! Authentication insertion point for Tizen pairing tokens.

use crate::error::Result;

/// Supplies the pairing token used for the remote-channel handshake.
///
/// Consumer TVs obtain this token through the on-screen PIN handshake implemented
/// in [`crate::TizenSession::connect`]: the TV returns the token inside the
/// `ms.channel.connect` reply and the application persists it for reconnects.
///
/// Enterprise / commercial panels instead require the token derived by the
/// reverse-engineered "token generation math" (TV MAC + hardcoded secret salt)
/// described in `docs/01-playbook.md` §10 Gap B, and 2024+ firmware additionally
/// encrypts WebSocket framing with AES-GCM (Gap D). Both are Phase 3 work, so the
/// default implementation below returns [`crate::TizenError::Unsupported`] rather than
/// panicking; callers can fall back to the on-screen handshake path.
pub trait TizenAuthProvider {
    /// Returns a device token for the remote channel.
    ///
    /// The default returns [`crate::TizenError::Unsupported`] because the required token
    /// math is blocked on playbook §10 Gap B (enterprise token-generation math) and
    /// gap D (AES-GCM encrypted framing). Override once that reverse-engineering
    /// output is available. This never panics.
    fn device_token(&self) -> Result<String> {
        Err(tv_rs_transport::Unsupported::new(
            3,
            "playbook §10 Gap B: enterprise token-generation math (TV MAC + secret salt) \
             requires REA decompilation of the vendor APK; consumer TVs instead use the \
             on-screen PIN handshake in TizenSession::connect, and 2024+ AES-GCM framing is \
             Gap D",
        )
        .into())
    }
}

#[cfg(test)]
mod tests {
    use super::TizenAuthProvider;
    use crate::error::TizenError;

    struct DefaultProvider;

    impl TizenAuthProvider for DefaultProvider {}

    #[test]
    fn default_device_token_is_unsupported_and_does_not_panic() {
        let provider = DefaultProvider;
        let err = provider
            .device_token()
            .expect_err("the default implementation must return Err, not panic");
        match err {
            TizenError::Unsupported(inner) => {
                assert_eq!(inner.phase, 3);
                assert!(
                    inner.reason.contains("Gap B"),
                    "reason must name Gap B: {}",
                    inner.reason
                );
            }
            other => panic!("unexpected error variant: {other:?}"),
        }
    }
}
