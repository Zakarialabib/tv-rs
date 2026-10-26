//! P2P Transport Layer using QUIC for high-speed digital signage asset transfer.
//!
//! Digital signage players and sender devices (e.g. Android mobile app and
//! Windows display player) require low-latency, high-throughput asset transfer
//! for large video files, images, and layout manifests. HTTP 206 and HLS are
//! ideal for media streaming/scrubbing, but lack the speed and multiplexing
//! required for rapid P2P asset synchronization.
//!
//! This module provides [`P2pEndpoint`] and [`P2pConnection`], which leverage
//! QUIC (`quinn`) to open multiplexed, encrypted streams for concurrent asset
//! delivery over UDP.

use bytes::{BufMut, BytesMut};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt};

/// Errors encountered during QUIC P2P asset transport operations.
#[derive(Debug, Error)]
pub enum P2pError {
    /// Failure during TLS configuration or certificate generation.
    #[error("TLS configuration error: {0}")]
    TlsConfig(String),

    /// Failure configuring QUIC endpoint or socket binding.
    #[error("Endpoint configuration error: {0}")]
    EndpointConfig(String),

    /// Connection error when contacting or accepting a P2P peer.
    #[error("Connection error: {0}")]
    Connection(String),

    /// Error reading or writing to a QUIC stream.
    #[error("Stream error: {0}")]
    Stream(String),

    /// Failed to serialize asset header metadata.
    #[error("Failed to serialize asset header: {0}")]
    HeaderSerialization(String),

    /// Failed to deserialize asset header metadata.
    #[error("Failed to deserialize asset header: {0}")]
    HeaderDeserialization(String),

    /// Transferred asset data checksum did not match expected checksum.
    #[error("Checksum mismatch: expected {expected}, calculated {actual}")]
    ChecksumMismatch {
        /// The expected SHA-256 hex checksum.
        expected: String,
        /// The actual SHA-256 hex checksum calculated from received bytes.
        actual: String,
    },

    /// Underlying I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Metadata header sent at the start of a QUIC stream before asset payload.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AssetHeader {
    /// Unique identifier for the asset.
    pub asset_id: String,

    /// Name or relative path of the asset (e.g. "promo_video.mp4").
    pub name: String,

    /// MIME content type (e.g. "video/mp4", "image/png").
    pub content_type: String,

    /// Total size in bytes of the asset payload following the header.
    pub total_size: u64,

    /// Optional SHA-256 checksum (hex-encoded) for verifying data integrity.
    pub sha256_checksum: Option<String>,

    /// Arbitrary key-value metadata (e.g. layout position, display group).
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

impl AssetHeader {
    /// Creates a new asset header without checksum or extra metadata.
    pub fn new(
        asset_id: impl Into<String>,
        name: impl Into<String>,
        content_type: impl Into<String>,
        total_size: u64,
    ) -> Self {
        Self {
            asset_id: asset_id.into(),
            name: name.into(),
            content_type: content_type.into(),
            total_size,
            sha256_checksum: None,
            metadata: HashMap::new(),
        }
    }

    /// Attaches a SHA-256 hex checksum calculated from `data`.
    pub fn with_checksum(mut self, data: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(data);
        let result = hasher.finalize();
        self.sha256_checksum = Some(hex::encode(result));
        self
    }

    /// Serializes header to JSON bytes prefixed with a 4-byte big-endian length.
    pub fn encode(&self) -> Result<Vec<u8>, P2pError> {
        let json =
            serde_json::to_vec(self).map_err(|e| P2pError::HeaderSerialization(e.to_string()))?;
        let len = u32::try_from(json.len())
            .map_err(|_| P2pError::HeaderSerialization("Header length exceeds u32::MAX".into()))?;

        let mut buf = BytesMut::with_capacity(4 + json.len());
        buf.put_u32(len);
        buf.put_slice(&json);
        Ok(buf.to_vec())
    }

    /// Deserializes header from stream by reading length prefix and JSON payload.
    pub async fn decode_from_stream<R>(reader: &mut R) -> Result<Self, P2pError>
    where
        R: AsyncRead + Unpin,
    {
        let mut len_buf = [0u8; 4];
        reader.read_exact(&mut len_buf).await.map_err(|e| {
            P2pError::HeaderDeserialization(format!("Failed to read header length: {e}"))
        })?;

        let len = u32::from_be_bytes(len_buf) as usize;
        if len > 10 * 1024 * 1024 {
            return Err(P2pError::HeaderDeserialization(
                "Header length exceeds maximum allowed limit (10MB)".into(),
            ));
        }

        let mut json_buf = vec![0u8; len];
        reader.read_exact(&mut json_buf).await.map_err(|e| {
            P2pError::HeaderDeserialization(format!("Failed to read header payload: {e}"))
        })?;

        serde_json::from_slice(&json_buf)
            .map_err(|e| P2pError::HeaderDeserialization(e.to_string()))
    }
}

mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        bytes.as_ref().iter().map(|b| format!("{b:02x}")).collect()
    }
}

/// Generates a self-signed TLS certificate and private key for P2P QUIC transport.
pub fn generate_p2p_cert(
    san_names: &[String],
) -> Result<
    (
        rustls::pki_types::CertificateDer<'static>,
        rustls::pki_types::PrivateKeyDer<'static>,
    ),
    P2pError,
> {
    let subject_alt_names = if san_names.is_empty() {
        vec!["localhost".to_string()]
    } else {
        san_names.to_vec()
    };

    let key_pair = rcgen::KeyPair::generate()
        .map_err(|e| P2pError::TlsConfig(format!("Failed to generate keypair: {e}")))?;

    let mut params = rcgen::CertificateParams::new(subject_alt_names)
        .map_err(|e| P2pError::TlsConfig(format!("Failed to setup cert params: {e}")))?;
    params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);

    let cert = params
        .self_signed(&key_pair)
        .map_err(|e| P2pError::TlsConfig(format!("Failed to sign certificate: {e}")))?;

    let cert_der = cert.der().to_vec();
    let key_der = key_pair.serialize_der();

    Ok((
        rustls::pki_types::CertificateDer::from(cert_der),
        rustls::pki_types::PrivateKeyDer::Pkcs8(rustls::pki_types::PrivatePkcs8KeyDer::from(
            key_der,
        )),
    ))
}

/// Custom dummy certificate verifier for P2P peer connection setup over QUIC.
#[derive(Debug)]
struct SkipServerVerification;

impl rustls::client::danger::ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::ED25519,
            rustls::SignatureScheme::RSA_PSS_SHA256,
        ]
    }
}

/// Configures QUIC ServerConfig with given certificate and private key.
pub fn create_p2p_server_config(
    cert: rustls::pki_types::CertificateDer<'static>,
    key: rustls::pki_types::PrivateKeyDer<'static>,
) -> Result<quinn::ServerConfig, P2pError> {
    let mut server_crypto = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert], key)
        .map_err(|e| P2pError::TlsConfig(format!("Failed to create server TLS config: {e}")))?;

    server_crypto.alpn_protocols = vec![b"tv-rs-p2p-signage/1".to_vec()];

    let server_config = quinn::ServerConfig::with_crypto(Arc::new(
        quinn::crypto::rustls::QuicServerConfig::try_from(server_crypto).map_err(|e| {
            P2pError::TlsConfig(format!("Failed to create QUIC server crypto config: {e}"))
        })?,
    ));

    Ok(server_config)
}

/// Configures QUIC ClientConfig for connecting to P2P peers.
pub fn create_p2p_client_config() -> Result<quinn::ClientConfig, P2pError> {
    let mut client_crypto = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(SkipServerVerification))
        .with_no_client_auth();

    client_crypto.alpn_protocols = vec![b"tv-rs-p2p-signage/1".to_vec()];

    let client_config = quinn::ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(client_crypto).map_err(|e| {
            P2pError::TlsConfig(format!("Failed to create QUIC client crypto config: {e}"))
        })?,
    ));

    Ok(client_config)
}

/// A P2P QUIC endpoint capable of hosting and connecting to peers.
pub struct P2pEndpoint {
    endpoint: quinn::Endpoint,
}

impl P2pEndpoint {
    /// Binds a new P2P QUIC endpoint on `bind_addr` with auto-generated self-signed TLS credentials.
    pub fn bind(bind_addr: SocketAddr) -> Result<Self, P2pError> {
        let (cert, key) = generate_p2p_cert(&[])?;
        let server_config = create_p2p_server_config(cert, key)?;
        let client_config = create_p2p_client_config()?;

        Self::bind_with_config(bind_addr, server_config, client_config)
    }

    /// Binds a new P2P QUIC endpoint with custom server and client configurations.
    pub fn bind_with_config(
        bind_addr: SocketAddr,
        server_config: quinn::ServerConfig,
        client_config: quinn::ClientConfig,
    ) -> Result<Self, P2pError> {
        let mut endpoint = quinn::Endpoint::server(server_config, bind_addr)
            .map_err(|e| P2pError::EndpointConfig(e.to_string()))?;
        endpoint.set_default_client_config(client_config);

        Ok(Self { endpoint })
    }

    /// Returns the local socket address this endpoint is bound to.
    pub fn local_addr(&self) -> Result<SocketAddr, P2pError> {
        self.endpoint
            .local_addr()
            .map_err(|e| P2pError::EndpointConfig(e.to_string()))
    }

    /// Connects to a remote P2P QUIC peer.
    pub async fn connect(
        &self,
        remote_addr: SocketAddr,
        server_name: &str,
    ) -> Result<P2pConnection, P2pError> {
        let connecting = self
            .endpoint
            .connect(remote_addr, server_name)
            .map_err(|e| P2pError::Connection(e.to_string()))?;

        let connection = connecting
            .await
            .map_err(|e| P2pError::Connection(e.to_string()))?;

        Ok(P2pConnection { connection })
    }

    /// Accepts an incoming connection from a P2P peer.
    pub async fn accept(&self) -> Option<P2pConnection> {
        let incoming = self.endpoint.accept().await?;
        let connection = incoming.await.ok()?;
        Some(P2pConnection { connection })
    }

    /// Closes the QUIC endpoint.
    pub fn close(&self) {
        self.endpoint.close(0u32.into(), b"endpoint closed");
    }
}

/// An established P2P QUIC connection with a peer.
#[derive(Clone)]
pub struct P2pConnection {
    connection: quinn::Connection,
}

impl P2pConnection {
    /// Returns the remote peer's socket address.
    pub fn remote_address(&self) -> SocketAddr {
        self.connection.remote_address()
    }

    /// Sends a complete asset payload to the remote peer over a dedicated QUIC stream.
    pub async fn send_asset(
        &self,
        header: &AssetHeader,
        data: &[u8],
        progress_tx: Option<tokio::sync::mpsc::Sender<u64>>,
    ) -> Result<(), P2pError> {
        let mut send_stream = self
            .connection
            .open_uni()
            .await
            .map_err(|e| P2pError::Stream(e.to_string()))?;

        let header_encoded = header.encode()?;
        send_stream
            .write_all(&header_encoded)
            .await
            .map_err(|e| P2pError::Stream(e.to_string()))?;

        let chunk_size = 64 * 1024;
        let mut offset = 0;
        while offset < data.len() {
            let end = (offset + chunk_size).min(data.len());
            let chunk = &data[offset..end];
            send_stream
                .write_all(chunk)
                .await
                .map_err(|e| P2pError::Stream(e.to_string()))?;
            offset = end;

            if let Some(ref tx) = progress_tx {
                let _ = tx.send(offset as u64).await;
            }
        }

        send_stream
            .finish()
            .map_err(|e| P2pError::Stream(e.to_string()))?;

        let _ = send_stream.stopped().await;

        Ok(())
    }

    /// Sends an asset stream from an async reader to the remote peer over a QUIC stream.
    pub async fn send_asset_stream<R>(
        &self,
        header: &AssetHeader,
        mut reader: R,
        progress_tx: Option<tokio::sync::mpsc::Sender<u64>>,
    ) -> Result<(), P2pError>
    where
        R: AsyncRead + Unpin,
    {
        let mut send_stream = self
            .connection
            .open_uni()
            .await
            .map_err(|e| P2pError::Stream(e.to_string()))?;

        let header_encoded = header.encode()?;
        send_stream
            .write_all(&header_encoded)
            .await
            .map_err(|e| P2pError::Stream(e.to_string()))?;

        let mut buf = vec![0u8; 64 * 1024];
        let mut total_written = 0u64;

        loop {
            let n = reader
                .read(&mut buf)
                .await
                .map_err(|e| P2pError::Stream(e.to_string()))?;

            if n == 0 {
                break;
            }

            send_stream
                .write_all(&buf[..n])
                .await
                .map_err(|e| P2pError::Stream(e.to_string()))?;

            total_written += n as u64;
            if let Some(ref tx) = progress_tx {
                let _ = tx.send(total_written).await;
            }
        }

        send_stream
            .finish()
            .map_err(|e| P2pError::Stream(e.to_string()))?;

        let _ = send_stream.stopped().await;

        Ok(())
    }

    /// Receives an asset from an incoming QUIC stream, reading metadata header and payload bytes.
    pub async fn receive_asset(&self) -> Result<(AssetHeader, Vec<u8>), P2pError> {
        let mut recv_stream = self
            .connection
            .accept_uni()
            .await
            .map_err(|e| P2pError::Stream(e.to_string()))?;

        let header = AssetHeader::decode_from_stream(&mut recv_stream).await?;

        let payload = recv_stream
            .read_to_end(header.total_size as usize)
            .await
            .map_err(|e| P2pError::Stream(e.to_string()))?;

        if let Some(ref expected_checksum) = header.sha256_checksum {
            let mut hasher = Sha256::new();
            hasher.update(&payload);
            let actual_checksum = hex::encode(hasher.finalize());

            if !actual_checksum.eq_ignore_ascii_case(expected_checksum) {
                return Err(P2pError::ChecksumMismatch {
                    expected: expected_checksum.clone(),
                    actual: actual_checksum,
                });
            }
        }

        Ok((header, payload))
    }

    /// Closes the QUIC connection with an error code and reason.
    pub fn close(&self, error_code: u32, reason: &[u8]) {
        self.connection.close(error_code.into(), reason);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_header_checksum_and_encoding_roundtrip() {
        let data = b"Hello, digital signage world!";
        let header = AssetHeader::new("asset_1", "banner.png", "image/png", data.len() as u64)
            .with_checksum(data);

        assert_eq!(header.asset_id, "asset_1");
        assert_eq!(header.name, "banner.png");
        assert_eq!(header.total_size, data.len() as u64);
        assert!(header.sha256_checksum.is_some());

        let encoded = header.encode().expect("encoding asset header");
        assert!(encoded.len() > 4);

        let mut slice = &encoded[..];
        let rt_header = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(AssetHeader::decode_from_stream(&mut slice))
            .expect("decoding asset header");

        assert_eq!(header, rt_header);
    }

    #[tokio::test]
    async fn p2p_cert_generation() {
        let (cert, _key) = generate_p2p_cert(&["localhost".into()]).expect("generating cert");
        assert!(!cert.is_empty());
    }

    #[tokio::test]
    async fn p2p_quic_transfer_single_asset() {
        let server_ep = P2pEndpoint::bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let server_addr = server_ep.local_addr().unwrap();

        let client_ep = P2pEndpoint::bind("127.0.0.1:0".parse().unwrap()).unwrap();

        let server_task = tokio::spawn(async move {
            let conn = server_ep.accept().await.expect("accept connection");
            let (header, payload) = conn.receive_asset().await.expect("receive asset");
            (header, payload)
        });

        let client_conn = client_ep
            .connect(server_addr, "localhost")
            .await
            .expect("client connect");

        let payload = vec![0xABu8; 128 * 1024]; // 128KB payload
        let header = AssetHeader::new("v1", "signage_4k.mp4", "video/mp4", payload.len() as u64)
            .with_checksum(&payload);

        let (tx, mut rx) = tokio::sync::mpsc::channel(10);
        let client_conn_clone = client_conn.clone();
        let send_task = tokio::spawn(async move {
            client_conn_clone
                .send_asset(&header, &payload, Some(tx))
                .await
                .expect("send asset");
        });

        let mut progress_updates = Vec::new();
        while let Some(bytes_sent) = rx.recv().await {
            progress_updates.push(bytes_sent);
        }

        send_task.await.unwrap();
        let (recv_header, recv_payload) = server_task.await.unwrap();

        assert_eq!(recv_header.asset_id, "v1");
        assert_eq!(recv_header.name, "signage_4k.mp4");
        assert_eq!(recv_payload.len(), 128 * 1024);
        assert_eq!(recv_payload, vec![0xABu8; 128 * 1024]);
        assert!(!progress_updates.is_empty());
    }

    #[tokio::test]
    async fn p2p_quic_transfer_multiplexed_concurrent_assets() {
        let server_ep = P2pEndpoint::bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let server_addr = server_ep.local_addr().unwrap();

        let client_ep = P2pEndpoint::bind("127.0.0.1:0".parse().unwrap()).unwrap();

        let server_task = tokio::spawn(async move {
            let conn = server_ep.accept().await.expect("accept connection");
            let mut received = Vec::new();
            for _ in 0..3 {
                let item = conn
                    .receive_asset()
                    .await
                    .expect("receive multiplexed asset");
                received.push(item);
            }
            received
        });

        let client_conn = client_ep
            .connect(server_addr, "localhost")
            .await
            .expect("client connect");

        let mut send_handles = Vec::new();
        for i in 0..3 {
            let payload = format!("Asset content payload index {i}").into_bytes();
            let header = AssetHeader::new(
                format!("asset_{i}"),
                format!("file_{i}.txt"),
                "text/plain",
                payload.len() as u64,
            )
            .with_checksum(&payload);

            let conn_clone = client_conn.clone();
            let handle = tokio::spawn(async move {
                conn_clone
                    .send_asset(&header, &payload, None)
                    .await
                    .expect("send asset");
            });
            send_handles.push(handle);
        }

        for handle in send_handles {
            handle.await.unwrap();
        }

        let received = server_task.await.unwrap();
        assert_eq!(received.len(), 3);
    }

    #[tokio::test]
    async fn p2p_quic_checksum_mismatch_fails() {
        let server_ep = P2pEndpoint::bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let server_addr = server_ep.local_addr().unwrap();

        let client_ep = P2pEndpoint::bind("127.0.0.1:0".parse().unwrap()).unwrap();

        let server_task = tokio::spawn(async move {
            let conn = server_ep.accept().await.expect("accept connection");
            conn.receive_asset().await
        });

        let client_conn = client_ep
            .connect(server_addr, "localhost")
            .await
            .expect("client connect");

        let correct_data = b"Correct asset data";
        let tampered_data = b"Tampered asset data";

        let header = AssetHeader::new(
            "err1",
            "bad.bin",
            "application/octet-stream",
            tampered_data.len() as u64,
        )
        .with_checksum(correct_data);

        client_conn
            .send_asset(&header, tampered_data, None)
            .await
            .expect("send asset");

        let recv_result = server_task.await.unwrap();
        assert!(matches!(
            recv_result,
            Err(P2pError::ChecksumMismatch { .. })
        ));
    }
}
