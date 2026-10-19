//! The TLS policy shared by the vendor WebSocket channels, and the connector built from it.
//!
//! Two backends are available and exactly one must be enabled:
//!
//! | feature | connector | when to choose it |
//! | --- | --- | --- |
//! | `rustls` (default) | [`tokio_tungstenite::Connector::Rustls`] | portable, no system TLS, no OpenSSL |
//! | `native-tls` | [`tokio_tungstenite::Connector::NativeTls`] | hosts that must use the platform trust store |
//!
//! Both honour [`TlsPolicy`], and every `danger_accept_invalid_*` equivalent is
//! reachable only from this module.

#[cfg(all(feature = "ws", feature = "rustls"))]
use std::sync::Arc;

#[cfg(all(feature = "ws", feature = "native-tls", not(feature = "rustls")))]
use native_tls::TlsConnector;
#[cfg(all(feature = "ws", feature = "rustls"))]
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
#[cfg(all(feature = "ws", feature = "rustls"))]
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
#[cfg(all(feature = "ws", feature = "rustls"))]
use rustls::{ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme};
#[cfg(feature = "ws")]
use thiserror::Error;
#[cfg(feature = "ws")]
use tokio_tungstenite::Connector;

#[cfg(all(feature = "ws", feature = "native-tls", feature = "rustls"))]
compile_error!(
    "tv-rs-transport: enable either the `rustls` or the `native-tls` TLS backend, not both"
);

#[cfg(all(feature = "ws", not(feature = "rustls"), not(feature = "native-tls")))]
compile_error!(
    "tv-rs-transport: the `ws` feature needs a TLS backend — enable `rustls` or `native-tls`"
);

/// TLS certificate-validation policy for a `wss://` remote channel.
///
/// [`TlsPolicy::AcceptInvalid`] is the Phase-1 interim standing in for the
/// certificate-pinning bypass described in `docs/01-playbook.md` §10 Gap A (\"SSL
/// Certificate Pinning & WSS Handshakes\"). It disables certificate *and* hostname
/// validation entirely and must NOT be used in production; it exists only until
/// the reverse-engineering phase supplies the correct self-signed CA material.
///
/// This type is defined once, here, rather than in each vendor crate: its
/// consequences are a security property of the whole workspace, so two copies
/// drifting apart was not an acceptable risk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TlsPolicy {
    /// Validate the TV certificate normally. This fails against the self-signed
    /// certificates shipped by retail TVs until the §10 Gap A pinning bypass lands.
    #[default]
    Strict,
    /// Accept invalid certificates and hostnames. Phase-1 interim only; unsafe.
    AcceptInvalid,
}

/// Failure while building the TLS connector.
#[cfg(feature = "ws")]
#[derive(Debug, Error)]
pub enum TlsError {
    /// The TLS backend refused to build a connector.
    #[error("failed to build the TLS connector: {0}")]
    Build(String),
}

/// Builds the connector to hand to `connect_async_tls_with_config`.
///
/// `tls == false` yields [`Connector::Plain`] — the legacy unencrypted WebSocket
/// channel. `tls == true` yields the connector of the enabled backend, honouring
/// `policy`.
///
/// Every accept-any-certificate path in the workspace is reachable only through
/// this function, so the escape hatch has exactly one definition and one place to
/// delete when the §10 Gap A pinning bypass lands.
#[cfg(all(feature = "ws", feature = "rustls"))]
pub fn connector(policy: TlsPolicy, tls: bool) -> Result<Connector, TlsError> {
    if !tls {
        return Ok(Connector::Plain);
    }
    Ok(Connector::Rustls(Arc::new(rustls_config(policy)?)))
}

/// Builds the connector to hand to `connect_async_tls_with_config`.
///
/// `tls == false` yields [`Connector::Plain`] — the legacy unencrypted WebSocket
/// channel. `tls == true` yields a `native-tls` connector honouring `policy`.
#[cfg(all(feature = "ws", feature = "native-tls", not(feature = "rustls")))]
pub fn connector(policy: TlsPolicy, tls: bool) -> Result<Connector, TlsError> {
    if !tls {
        return Ok(Connector::Plain);
    }

    let mut builder = TlsConnector::builder();
    if policy == TlsPolicy::AcceptInvalid {
        builder.danger_accept_invalid_certs(true);
        builder.danger_accept_invalid_hostnames(true);
    }
    let connector = builder
        .build()
        .map_err(|error| TlsError::Build(error.to_string()))?;
    Ok(Connector::NativeTls(connector))
}

/// Builds a rustls client config for `policy`.
///
/// `Strict` verifies against the platform trust store. `AcceptInvalid` installs
/// [`AcceptAnyTvCert`], which is the rustls equivalent of
/// `danger_accept_invalid_certs` and exists only for TVs that present a
/// self-signed certificate.
#[cfg(all(feature = "ws", feature = "rustls"))]
pub fn rustls_config(policy: TlsPolicy) -> Result<ClientConfig, TlsError> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let builder = ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()
        .map_err(|error| TlsError::Build(error.to_string()))?;

    let builder = match policy {
        TlsPolicy::AcceptInvalid => builder
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(AcceptAnyTvCert(provider))),
        TlsPolicy::Strict => {
            // Platform trust store; TVs with self-signed certs will (correctly)
            // fail here until the pinning bypass lands.
            let mut roots = RootCertStore::empty();
            let native = rustls_native_certs::load_native_certs();
            for cert in native.certs {
                // Malformed entries in an OS store are not fatal for us.
                let _ = roots.add(cert);
            }
            builder.with_root_certificates(roots)
        }
    };

    Ok(builder.with_no_client_auth())
}

/// Verifier that accepts any server certificate — the rustls form of
/// `danger_accept_invalid_certs`.
///
/// Signature checks still run, so this accepts an *untrusted* certificate but not
/// a broken handshake. Only reachable through [`TlsPolicy::AcceptInvalid`].
#[cfg(all(feature = "ws", feature = "rustls"))]
#[derive(Debug)]
struct AcceptAnyTvCert(Arc<rustls::crypto::CryptoProvider>);

#[cfg(all(feature = "ws", feature = "rustls"))]
impl ServerCertVerifier for AcceptAnyTvCert {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

#[cfg(all(test, feature = "ws"))]
mod tests {
    use tokio_tungstenite::Connector;

    use super::{connector, TlsPolicy};

    #[test]
    fn policy_defaults_to_strict() {
        assert_eq!(TlsPolicy::default(), TlsPolicy::Strict);
    }

    #[test]
    fn plain_channel_ignores_the_policy() {
        let plain = connector(TlsPolicy::AcceptInvalid, false).expect("plain connector");
        assert!(matches!(plain, Connector::Plain));
    }

    #[cfg(feature = "rustls")]
    #[test]
    fn tls_channel_builds_a_rustls_connector() {
        let tls = connector(TlsPolicy::Strict, true).expect("rustls connector");
        assert!(matches!(tls, Connector::Rustls(_)));
    }

    #[cfg(all(feature = "native-tls", not(feature = "rustls")))]
    #[test]
    fn tls_channel_builds_a_native_tls_connector() {
        let tls = connector(TlsPolicy::Strict, true).expect("native-tls connector");
        assert!(matches!(tls, Connector::NativeTls(_)));
    }

    #[cfg(feature = "rustls")]
    #[test]
    fn accept_invalid_still_builds_a_config() {
        let config = super::rustls_config(TlsPolicy::AcceptInvalid).expect("permissive config");
        assert!(config.alpn_protocols.is_empty());
    }
}
