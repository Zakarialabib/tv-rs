//! Host-application authentication hook for LG webOS SSAP.
//!
//! The SSAP pairing `client-key` exchange itself is implemented for real in
//! [`crate::client::WebosSession`]; the plaintext registration handshake performed
//! by [`crate::client::WebosClient::connect`] returns the key issued by the TV.
//! Where that key is stored is a host-application concern, so this trait leaves
//! persistence to the caller. The TLS certificate-pinning bypass for port `3001`
//! remains Phase 3 (`docs/02-playbook.md` §10, TLS certificate pinning).

use crate::error::Result;

/// Supplies and persists the SSAP `client-key` for a host application.
///
/// Implement this trait when the application has its own credential store; the
/// default [`WebosAuthProvider::client_key`] reports "no key yet" and the default
/// [`WebosAuthProvider::persist_client_key`] reports that persistence is not
/// configured. Neither default ever panics.
pub trait WebosAuthProvider {
    /// Returns a previously stored `client-key`, or `None` when unknown.
    ///
    /// The default implementation returns `Ok(None)`.
    fn client_key(&self) -> Result<Option<String>> {
        Ok(None)
    }

    /// Stores the `client-key` issued by the TV during pairing.
    ///
    /// The default implementation returns [`crate::WebosError::Unsupported`] because the
    /// persistence location is a host-application decision and, together with the
    /// port `3001` certificate-pinning bypass, is deferred to Phase 3
    /// (`docs/02-playbook.md` §10, TLS certificate pinning).
    fn persist_client_key(&mut self, client_key: &str) -> Result<()> {
        let _ = client_key;
        Err(tv_rs_transport::Unsupported::new(
            3,
            "client-key persistence is a host-application concern; TLS certificate pinning for port 3001 is Phase 3 (playbook §10)",
        )
        .into())
    }
}

#[cfg(test)]
mod tests {
    use super::WebosAuthProvider;
    use crate::error::WebosError;

    /// A host application that has not configured any credential store.
    struct UnconfiguredAuth;

    impl WebosAuthProvider for UnconfiguredAuth {}

    #[test]
    fn default_client_key_is_none() {
        let auth = UnconfiguredAuth;
        assert_eq!(auth.client_key().expect("default never errors"), None);
    }

    #[test]
    fn default_persist_client_key_is_unsupported_and_does_not_panic() {
        let mut auth = UnconfiguredAuth;
        let result = auth.persist_client_key("abc123");
        match result {
            Err(WebosError::Unsupported(inner)) => assert_eq!(inner.phase, 3),
            other => panic!("expected Unsupported(phase 3), got {other:?}"),
        }
    }
}
