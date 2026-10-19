//! The TLS policy shared by the vendor WebSocket channels, and the connector built from it.

#[cfg(feature = "ws")]
use native_tls::TlsConnector;
#[cfg(feature = "ws")]
use thiserror::Error;
#[cfg(feature = "ws")]
use tokio_tungstenite::Connector;

/// TLS certificate-validation policy for a `wss://` remote channel.
///
/// [`TlsPolicy::AcceptInvalid`] is the Phase-1 interim standing in for the
/// certificate-pinning bypass described in `docs/01-playbook.md` §10 Gap A ("SSL
/// Certificate Pinning & WSS Handshakes"). It disables certificate *and* hostname
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

/// Failure while building a `native-tls` connector.
#[cfg(feature = "ws")]
#[derive(Debug, Error)]
pub enum TlsError {
    /// `native-tls` refused to build the connector.
    #[error("failed to build the native-tls connector: {0}")]
    Build(String),
}

/// Builds the connector to hand to `connect_async_tls_with_config`.
///
/// `tls == false` yields [`Connector::Plain`] — the legacy unencrypted WebSocket
/// channel. `tls == true` yields a `native-tls` connector honouring `policy`.
///
/// Every `danger_accept_invalid_*` call in the workspace is reachable only
/// through this function, so the `AcceptInvalid` escape hatch has exactly one
/// definition and one place to delete when the §10 Gap A pinning bypass lands.
#[cfg(feature = "ws")]
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

    #[test]
    fn tls_channel_builds_a_native_tls_connector() {
        let tls = connector(TlsPolicy::Strict, true).expect("native-tls connector");
        assert!(matches!(tls, Connector::NativeTls(_)));
    }
}
