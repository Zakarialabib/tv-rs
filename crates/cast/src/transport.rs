//! TLS + EKE2 frame transport handling Length-Prefixed protobufs and Cast Messages.

use crate::error::Result;
use crate::proto::cast_channel::CastMessage;
use bytes::{BufMut, BytesMut};
use pki_types::{CertificateDer, ServerName, UnixTime};
use prost::Message;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::{DigitallySignedStruct, SignatureScheme};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::{rustls, TlsConnector};

/// Default Cast port.
pub const CAST_PORT: u16 = 8009;

/// Frame reader and writer for Cast protocol over TLS.
pub struct CastTransport {
    stream: tokio_rustls::client::TlsStream<TcpStream>,
}

impl CastTransport {
    /// Connects to a Cast device and performs the TLS handshake.
    /// The Cast device uses a self-signed throwaway certificate, so we must accept any cert.
    pub async fn connect(ip: &str) -> Result<Self> {
        let addr = format!("{}:{}", ip, CAST_PORT);
        let stream = TcpStream::connect(addr).await?;

        // Disable certificate verification (Cast devices use self-signed certificates).
        let mut config = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(NoVerify))
            .with_no_client_auth();
        // Allow fallback to TLS 1.2 if needed by older casts.
        config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

        let connector = TlsConnector::from(Arc::new(config));

        let domain = ServerName::try_from(ip.to_string())
            .unwrap_or_else(|_| ServerName::try_from("localhost").unwrap());

        let stream = connector.connect(domain, stream).await?;

        Ok(Self { stream })
    }

    /// Reads a length-prefixed CastMessage.
    pub async fn read_message(&mut self) -> Result<CastMessage> {
        let mut length_bytes = [0u8; 4];
        self.stream.read_exact(&mut length_bytes).await?;
        let length = u32::from_be_bytes(length_bytes) as usize;

        let mut payload = vec![0u8; length];
        self.stream.read_exact(&mut payload).await?;

        let message = CastMessage::decode(&payload[..])?;
        Ok(message)
    }

    /// Writes a length-prefixed CastMessage.
    pub async fn write_message(&mut self, message: &CastMessage) -> Result<()> {
        let mut payload = BytesMut::new();
        message
            .encode(&mut payload)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let length = payload.len() as u32;
        let mut buf = BytesMut::with_capacity(4 + payload.len());
        buf.put_u32(length);
        buf.put_slice(&payload);

        self.stream.write_all(&buf).await?;
        self.stream.flush().await?;
        Ok(())
    }
}

/// A certificate verifier that accepts anything (for Cast's throwaway certs).
#[derive(Debug)]
struct NoVerify;

impl ServerCertVerifier for NoVerify {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::RSA_PKCS1_SHA1,
            SignatureScheme::ECDSA_SHA1_Legacy,
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
            SignatureScheme::ECDSA_NISTP521_SHA512,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::ED25519,
            SignatureScheme::ED448,
        ]
    }
}
