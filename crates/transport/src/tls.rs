//! The TLS policy shared by the vendor WebSocket channels, and the connector built from it.
//!
//! Two backends are available and exactly one must be enabled:
//!
//! | feature | connector | when to choose it |
//! | --- | --- | --- |
//! | `rustls` (default) | `Connector::Rustls` | portable, no system TLS, no OpenSSL |
//! | `native-tls` | `Connector::NativeTls` | hosts that must use the platform trust store |
//!
//! Both honour [`TlsPolicy`], and every `danger_accept_invalid_*` equivalent is
//! reachable only from this module.
//!
//! # Self-signed TV certificates
//!
//! Retail TVs present a certificate no public CA signed, so [`TlsPolicy::Strict`]
//! — the default — rejects them. Two policies make them usable without disabling
//! validation:
//!
//! * [`TlsPolicy::TrustOnFirstUse`] accepts the certificate the TV presents on the
//!   first connection and records its [`CertFingerprint`]. Persist that value and
//!   use [`TlsPolicy::Pinned`] from then on.
//! * [`TlsPolicy::Pinned`] accepts exactly one certificate and nothing else.
//!
//! Both are implemented by verifying the attacker-controlled part of the handshake
//! the same way a CA-signed chain would be checked: the certificate *is* the
//! identity, and a fingerprint is a 32-byte identity. Losing the pin means
//! re-pairing, which is the correct failure mode for a device on your own LAN.

//! # Recording a pin
//!
//! ```no_run
//! use tv_rs_transport::{cert_sink, TlsPolicy};
//!
//! // First connection to a TV that has never been seen.
//! let policy = TlsPolicy::TrustOnFirstUse(cert_sink());
//! // … connect: the handshake accepts and records the certificate.
//!
//! if let Some(fingerprint) = policy.recorded_fingerprint() {
//!     std::fs::write("living-room.pin", fingerprint.to_hex()).expect("persist");
//! }
//!
//! // Every later connection verifies that exact certificate.
//! let stored = std::fs::read_to_string("living-room.pin").expect("read");
//! let policy = TlsPolicy::pinned(stored.trim()).expect("valid fingerprint");
//! ```
//!
use std::fmt;
#[cfg(all(feature = "ws", feature = "rustls"))]
use std::sync::{Arc, Mutex};

#[cfg(all(feature = "ws", feature = "native-tls", not(feature = "rustls")))]
use native_tls::TlsConnector;
#[cfg(all(feature = "ws", feature = "rustls"))]
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
#[cfg(all(feature = "ws", feature = "rustls"))]
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
#[cfg(all(feature = "ws", feature = "rustls"))]
use rustls::{ClientConfig, DigitallySignedStruct, SignatureScheme};
// Only the `Strict` policy's non-feature path builds a root store by hand; with
// `platform-verifier` the operating system supplies one, so the import would be
// unused — and an unused import is a clippy failure under `-D warnings`.
#[cfg(not(feature = "platform-verifier"))]
use rustls::RootCertStore;
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

/// SHA-256 fingerprint of a TV's certificate.
///
/// This is the identity a pinned channel verifies against. It is derived from the
/// DER certificate the TV presents, so it changes when the TV's firmware
/// regenerates its certificate — at which point the connection fails and the
/// caller re-runs the trust-on-first-use step.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct CertFingerprint([u8; 32]);

impl CertFingerprint {
    /// Computes the fingerprint of a DER-encoded certificate.
    #[cfg(all(feature = "ws", feature = "rustls"))]
    #[must_use]
    pub fn of_der(cert: &[u8]) -> Self {
        use sha2::{Digest, Sha256};
        let digest = Sha256::digest(cert);
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(&digest);
        Self(bytes)
    }

    /// The raw 32 bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Lowercase hex, the form to store in config or print in a log.
    #[must_use]
    pub fn to_hex(&self) -> String {
        let mut out = String::with_capacity(64);
        for byte in self.0 {
            out.push(char::from(HEX[usize::from(byte >> 4)]));
            out.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
        out
    }

    /// Parses the hex form produced by [`CertFingerprint::to_hex`].
    ///
    /// Colon separators are accepted because that is how people copy
    /// fingerprints out of `openssl x509 -fingerprint`.
    pub fn from_hex(text: &str) -> Result<Self, TlsError> {
        let compact: String = text
            .chars()
            .filter(|c| !matches!(c, ':' | ' ' | '-' | '_'))
            .collect();
        if compact.len() != 64 {
            return Err(TlsError::BadFingerprint(format!(
                "expected 64 hex characters, got {}",
                compact.len()
            )));
        }
        let mut bytes = [0u8; 32];
        for (index, chunk) in compact.as_bytes().as_chunks::<2>().0.iter().enumerate() {
            let pair = std::str::from_utf8(chunk)
                .map_err(|_| TlsError::BadFingerprint("not utf-8".to_string()))?;
            bytes[index] = u8::from_str_radix(pair, 16)
                .map_err(|_| TlsError::BadFingerprint(format!("'{pair}' is not hex")))?;
        }
        Ok(Self(bytes))
    }
}

const HEX: &[u8; 16] = b"0123456789abcdef";

impl fmt::Debug for CertFingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CertFingerprint({})", self.to_hex())
    }
}

impl fmt::Display for CertFingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

/// Where [`TlsPolicy::TrustOnFirstUse`] records the certificate it observed.
///
/// One sink is meant to live as long as one client: the first handshake fills it,
/// every later handshake on the same sink must match what is already there.
#[cfg(all(feature = "ws", feature = "rustls"))]
pub type CertSink = Arc<Mutex<Option<CertFingerprint>>>;

/// Creates an empty [`CertSink`].
#[cfg(all(feature = "ws", feature = "rustls"))]
#[must_use]
pub fn cert_sink() -> CertSink {
    Arc::new(Mutex::new(None))
}

/// TLS certificate-validation policy for a `wss://` remote channel.
///
/// Defaults to [`TlsPolicy::Strict`]. The two pinning variants exist because TVs
/// ship self-signed certificates; [`TlsPolicy::AcceptInvalid`] exists only for
/// debugging a handshake and is never needed by application code.
///
/// This type is defined once, here, rather than in each vendor crate: its
/// consequences are a security property of the whole workspace, so two copies
/// drifting apart was not an acceptable risk.
#[derive(Clone, Default)]
pub enum TlsPolicy {
    /// Validate the TV certificate against the platform trust store.
    #[default]
    Strict,
    /// Accept exactly the certificate with this fingerprint.
    Pinned(CertFingerprint),
    /// Accept the certificate presented now and record it in the sink; reject any
    /// later certificate that does not match what was recorded.
    #[cfg(all(feature = "ws", feature = "rustls"))]
    TrustOnFirstUse(CertSink),
    /// Accept invalid certificates and hostnames. Debugging only; unsafe.
    AcceptInvalid,
}

impl fmt::Debug for TlsPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Strict => f.write_str("Strict"),
            Self::Pinned(fingerprint) => write!(f, "Pinned({fingerprint})"),
            #[cfg(all(feature = "ws", feature = "rustls"))]
            Self::TrustOnFirstUse(sink) => {
                let recorded = sink.lock().ok().and_then(|guard| *guard);
                match recorded {
                    Some(fingerprint) => write!(f, "TrustOnFirstUse({fingerprint})"),
                    None => f.write_str("TrustOnFirstUse(unrecorded)"),
                }
            }
            Self::AcceptInvalid => f.write_str("AcceptInvalid"),
        }
    }
}

impl PartialEq for TlsPolicy {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Strict, Self::Strict) | (Self::AcceptInvalid, Self::AcceptInvalid) => true,
            (Self::Pinned(left), Self::Pinned(right)) => left == right,
            #[cfg(all(feature = "ws", feature = "rustls"))]
            (Self::TrustOnFirstUse(left), Self::TrustOnFirstUse(right)) => Arc::ptr_eq(left, right),
            _ => false,
        }
    }
}

impl Eq for TlsPolicy {}

impl TlsPolicy {
    /// Convenience for the documented two-step flow: parse a stored fingerprint.
    pub fn pinned(hex: &str) -> Result<Self, TlsError> {
        Ok(Self::Pinned(CertFingerprint::from_hex(hex)?))
    }

    /// The fingerprint a [`TlsPolicy::TrustOnFirstUse`] policy recorded, if any.
    #[cfg(all(feature = "ws", feature = "rustls"))]
    #[must_use]
    pub fn recorded_fingerprint(&self) -> Option<CertFingerprint> {
        match self {
            Self::TrustOnFirstUse(sink) => sink.lock().ok().and_then(|guard| *guard),
            Self::Pinned(fingerprint) => Some(*fingerprint),
            _ => None,
        }
    }
}

/// Failure while building the TLS connector or parsing a fingerprint.
#[derive(Debug, Error)]
pub enum TlsError {
    /// The TLS backend refused to build a connector.
    #[error("failed to build the TLS connector: {0}")]
    Build(String),
    /// A fingerprint string was not 32 hex-encoded bytes.
    #[error("invalid certificate fingerprint: {0}")]
    BadFingerprint(String),
    /// Pinning needs the rustls backend; `native-tls` cannot verify a custom
    /// fingerprint through `tokio-tungstenite`'s connector.
    #[error("certificate pinning requires the `rustls` backend (this build uses `native-tls`)")]
    PinningUnavailable,
}

/// Builds the connector to hand to `connect_async_tls_with_config`.
///
/// `tls == false` yields [`Connector::Plain`] — the legacy unencrypted WebSocket
/// channel. `tls == true` yields the connector of the enabled backend, honouring
/// `policy`.
///
/// Every accept-any-certificate path in the workspace is reachable only through
/// this function, so the escape hatches have exactly one definition.
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
    match policy {
        TlsPolicy::AcceptInvalid => {
            builder.danger_accept_invalid_certs(true);
            builder.danger_accept_invalid_hostnames(true);
        }
        TlsPolicy::Pinned(_) | TlsPolicy::TrustOnFirstUse(_) => {
            return Err(TlsError::PinningUnavailable);
        }
        TlsPolicy::Strict => {}
    }
    let connector = builder
        .build()
        .map_err(|error| TlsError::Build(error.to_string()))?;
    Ok(Connector::NativeTls(connector))
}

/// Builds a rustls client config for `policy`.
///
/// `Strict` verifies against the platform trust store. `Pinned` and
/// `TrustOnFirstUse` install a fingerprint verifier; `AcceptInvalid` installs an
/// accept-any-cert verifier.
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
        TlsPolicy::Pinned(fingerprint) => {
            builder
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(FingerprintVerifier::new(
                    Some(fingerprint),
                    None,
                    provider,
                )))
        }
        TlsPolicy::TrustOnFirstUse(sink) => {
            let expected = sink.lock().ok().and_then(|guard| *guard);
            builder
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(FingerprintVerifier::new(
                    expected,
                    Some(sink),
                    provider,
                )))
        }
        TlsPolicy::Strict => {
            // Platform trust store; TVs with self-signed certs fail here by design.
            //
            // Two ways to get it, and the feature decides which. `rustls-native-certs`
            // reads a *system store*, which Android (a keystore) and iOS (its own
            // roots) do not expose that way — so on those platforms this arm would
            // find nothing to verify against. With `platform-verifier` the operating
            // system is asked directly instead, and rustls stays the stack.
            //
            // Only this arm changes. `Pinned` and `TrustOnFirstUse` install their own
            // verifiers above and never reach here, so the feature cannot weaken
            // pinning — which is the property the rustls-only policy exists to keep.
            #[cfg(feature = "platform-verifier")]
            {
                use rustls_platform_verifier::BuilderVerifierExt;

                // The extension takes this builder, so the ring provider and the
                // protocol versions chosen above are carried into the verifier.
                builder
                    .with_platform_verifier()
                    .map_err(|error| TlsError::Build(error.to_string()))?
            }

            #[cfg(not(feature = "platform-verifier"))]
            {
                let mut roots = RootCertStore::empty();
                let native = rustls_native_certs::load_native_certs();
                for cert in native.certs {
                    // Malformed entries in an OS store are not fatal for us.
                    let _ = roots.add(cert);
                }
                builder.with_root_certificates(roots)
            }
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

/// Verifier that compares the presented certificate against a SHA-256 fingerprint.
///
/// `expected` is the pin. When it is `None` the policy is trust-on-first-use: the
/// presented fingerprint is written into `sink` and the handshake proceeds. A
/// later handshake through the same sink sees a non-empty `expected` and is
/// verified like a normal pin.
#[cfg(all(feature = "ws", feature = "rustls"))]
#[derive(Debug)]
struct FingerprintVerifier {
    expected: Option<CertFingerprint>,
    sink: Option<CertSink>,
    provider: Arc<rustls::crypto::CryptoProvider>,
}

#[cfg(all(feature = "ws", feature = "rustls"))]
impl FingerprintVerifier {
    fn new(
        expected: Option<CertFingerprint>,
        sink: Option<CertSink>,
        provider: Arc<rustls::crypto::CryptoProvider>,
    ) -> Self {
        Self {
            expected,
            sink,
            provider,
        }
    }

    /// Compares (or records) the fingerprint, returning the rustls verdict.
    fn check(&self, end_entity: &CertificateDer<'_>) -> Result<ServerCertVerified, rustls::Error> {
        let observed = CertFingerprint::of_der(end_entity.as_ref());
        match self.expected {
            Some(expected) if expected != observed => {
                return Err(rustls::Error::General(format!(
                    "certificate fingerprint mismatch: expected {expected}, TV presented {observed}"
                )));
            }
            Some(_) => {}
            None => {
                if let Some(sink) = &self.sink {
                    // A poisoned lock means another thread panicked mid-write; the
                    // safe response is to fail the handshake rather than trust it.
                    match sink.lock() {
                        Ok(mut slot) => {
                            if let Some(recorded) = *slot {
                                if recorded != observed {
                                    return Err(rustls::Error::General(format!(
                                        "certificate fingerprint changed: recorded {recorded}, TV presented {observed}"
                                    )));
                                }
                            } else {
                                *slot = Some(observed);
                            }
                        }
                        Err(_) => {
                            return Err(rustls::Error::General(
                                "certificate sink lock poisoned".to_string(),
                            ));
                        }
                    }
                }
            }
        }
        Ok(ServerCertVerified::assertion())
    }
}

#[cfg(all(feature = "ws", feature = "rustls"))]
impl ServerCertVerifier for FingerprintVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        self.check(end_entity)
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
            &self.provider.signature_verification_algorithms,
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
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

#[cfg(all(test, feature = "ws"))]
mod tests {
    /// The feature's whole contract: it changes where `Strict` gets its roots and
    /// nothing else. A feature that also moved `Pinned` or `TrustOnFirstUse` would
    /// silently weaken pinning, which is the property the rustls-only policy exists
    /// to keep — so this asserts all four policies still build with it enabled.
    #[cfg(feature = "platform-verifier")]
    #[test]
    fn the_platform_verifier_changes_only_the_strict_policy() {
        assert!(
            rustls_config(TlsPolicy::Strict).is_ok(),
            "Strict asks the OS for roots"
        );
        assert!(rustls_config(TlsPolicy::Pinned(CertFingerprint::of_der(&[1, 2, 3]))).is_ok());
        assert!(rustls_config(TlsPolicy::TrustOnFirstUse(cert_sink())).is_ok());
        assert!(rustls_config(TlsPolicy::AcceptInvalid).is_ok());
    }

    use tokio_tungstenite::Connector;

    use super::{connector, TlsPolicy};
    // Gated with the test that needs them: with the feature off these would be
    // unused imports, and clippy runs `-D warnings`.
    #[cfg(feature = "platform-verifier")]
    use super::{cert_sink, rustls_config, CertFingerprint};

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

    #[cfg(feature = "rustls")]
    #[test]
    fn fingerprint_hex_round_trip() {
        let fingerprint = super::CertFingerprint::of_der(b"not really a certificate");
        let hex = fingerprint.to_hex();
        assert_eq!(hex.len(), 64);
        assert_eq!(
            super::CertFingerprint::from_hex(&hex).expect("parse hex"),
            fingerprint
        );

        // Colons and upper case, as copied out of openssl.
        let pretty = hex
            .to_uppercase()
            .as_bytes()
            .chunks(2)
            .map(|pair| std::str::from_utf8(pair).expect("utf-8").to_string())
            .collect::<Vec<_>>()
            .join(":");
        assert_eq!(
            super::CertFingerprint::from_hex(&pretty).expect("parse colon hex"),
            fingerprint
        );
    }

    #[cfg(feature = "rustls")]
    #[test]
    fn bad_fingerprints_are_rejected() {
        assert!(super::CertFingerprint::from_hex("abc").is_err());
        assert!(super::CertFingerprint::from_hex(&"zz".repeat(32)).is_err());
    }

    #[cfg(feature = "rustls")]
    #[test]
    fn pinned_policy_compares_fingerprints() {
        let left = super::CertFingerprint::of_der(b"left");
        let right = super::CertFingerprint::of_der(b"right");
        assert_eq!(TlsPolicy::Pinned(left), TlsPolicy::Pinned(left));
        assert_ne!(TlsPolicy::Pinned(left), TlsPolicy::Pinned(right));
        assert_ne!(TlsPolicy::Pinned(left), TlsPolicy::Strict);
    }

    #[cfg(feature = "rustls")]
    #[test]
    fn trust_on_first_use_records_then_enforces() {
        use rustls::client::danger::ServerCertVerifier;
        use rustls::pki_types::CertificateDer;

        let sink = super::cert_sink();
        let provider = std::sync::Arc::new(rustls::crypto::ring::default_provider());
        let policy = TlsPolicy::TrustOnFirstUse(std::sync::Arc::clone(&sink));
        assert_eq!(policy.recorded_fingerprint(), None);

        let first = super::FingerprintVerifier::new(
            None,
            Some(std::sync::Arc::clone(&sink)),
            provider.clone(),
        );
        let cert = CertificateDer::from(vec![0x30, 0x82, 0x01]);
        let name = rustls::pki_types::ServerName::try_from("192.168.1.10")
            .expect("server name")
            .to_owned();

        assert!(first
            .verify_server_cert(&cert, &[], &name, &[], rustls::pki_types::UnixTime::now())
            .is_ok());
        let recorded = policy.recorded_fingerprint().expect("fingerprint recorded");
        assert_eq!(recorded, super::CertFingerprint::of_der(cert.as_ref()));

        // Second connection: the same certificate still passes.
        let again = super::FingerprintVerifier::new(
            Some(recorded),
            Some(std::sync::Arc::clone(&sink)),
            provider.clone(),
        );
        assert!(again
            .verify_server_cert(&cert, &[], &name, &[], rustls::pki_types::UnixTime::now())
            .is_ok());

        // A different certificate is refused, and the pin is not overwritten.
        let impostor = CertificateDer::from(vec![0x30, 0x82, 0x02]);
        let mismatch = super::FingerprintVerifier::new(
            Some(recorded),
            Some(std::sync::Arc::clone(&sink)),
            provider,
        );
        assert!(mismatch
            .verify_server_cert(
                &impostor,
                &[],
                &name,
                &[],
                rustls::pki_types::UnixTime::now()
            )
            .is_err());
        assert_eq!(policy.recorded_fingerprint(), Some(recorded));
    }
}
