//! High-level ATR v2 client: TLS setup, Polo pairing, and control session.

use crate::control::ControlSession;
use crate::error::{AtrError, Result};
use crate::pairing::{parse_device_identity, DeviceIdentity};
use crate::proto::polo::{
    options, outer_message, Configuration, Options, OuterMessage, PairingRequest, PairingRequestAck,
};
use prost::Message;
use rsa::pkcs8::EncodePrivateKey;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, ClientConnection, StreamOwned};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Arc;

/// A verifier that accepts any server certificate (the ATR v2 pairing port
/// uses self-signed certs; the client pins them implicitly by using the
/// server cert in the secret computation).
#[derive(Debug)]
struct DangerousNoVerify;

impl ServerCertVerifier for DangerousNoVerify {
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
        _dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA1,
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::RSA_PKCS1_SHA384,
            rustls::SignatureScheme::RSA_PKCS1_SHA512,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::RSA_PSS_SHA256,
            rustls::SignatureScheme::RSA_PSS_SHA384,
            rustls::SignatureScheme::RSA_PSS_SHA512,
        ]
    }
}

/// Build a rustls client config that accepts any server cert and presents
/// a client certificate.
fn build_tls_config(client_cert: Vec<u8>, client_key: Vec<u8>) -> Result<Arc<ClientConfig>> {
    let key = rustls::pki_types::PrivateKeyDer::try_from(client_key)
        .map_err(|e| AtrError::PairingRejected(format!("bad client key: {e}")))?;

    let config = ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(DangerousNoVerify))
        .with_client_auth_cert(vec![CertificateDer::from(client_cert)], key)
        .map_err(|e| AtrError::PairingRejected(format!("client auth: {e}")))?;

    Ok(Arc::new(config))
}

/// Generate a self-signed client certificate for pairing.
///
/// Returns `(cert_der, key_der)` in DER format.
pub fn generate_client_cert() -> Result<(Vec<u8>, Vec<u8>)> {
    // Polo's secret is computed from the RSA modulus/exponent of both certs,
    // so the client key must be RSA (the TV's is always RSA). The `rsa` crate
    // generates it in pure Rust: rcgen's own RSA keygen needs aws-lc-rs (and a
    // NASM toolchain on Windows), while ring can sign an RSA key it did not
    // generate.
    let private_key = {
        let mut rng = rand::thread_rng();
        rsa::RsaPrivateKey::new(&mut rng, 2048)
            .map_err(|e| AtrError::PairingRejected(format!("key generation failed: {e}")))?
    };
    let pkcs8 = private_key
        .to_pkcs8_der()
        .map_err(|e| AtrError::PairingRejected(format!("pkcs8 encode: {e}")))?;
    let key_pair = rcgen::KeyPair::from_pkcs8_der_and_sign_algo(
        &pki_types::PrivatePkcs8KeyDer::from(pkcs8.as_bytes()),
        &rcgen::PKCS_RSA_SHA256,
    )
    .map_err(|e| AtrError::PairingRejected(format!("key pair from pkcs8: {e}")))?;
    let params = rcgen::CertificateParams::new(vec!["tv-rs-androidtv".into()])
        .map_err(|e| AtrError::PairingRejected(format!("cert params: {e}")))?;
    let cert = params
        .self_signed(&key_pair)
        .map_err(|e| AtrError::PairingRejected(format!("self-sign: {e}")))?;

    Ok((cert.der().to_vec(), key_pair.serialize_der()))
}

/// The ATR v2 client.
///
/// # Example
///
/// ```no_run
/// use tv_rs_androidtv::{AtrClient, Key};
///
/// # fn run() -> tv_rs_androidtv::Result<()> {
/// let mut client = AtrClient::new("192.168.1.20");
/// let identity = client.pair("tv-rs", "123456")?;
/// println!("Paired with {} ({})", identity.name, identity.mac);
/// let mut control = client.connect_control()?;
/// control.press_key(Key::Home)?;
/// # Ok(())
/// # }
/// ```
pub struct AtrClient {
    host: String,
    pair_port: u16,
    control_port: u16,
    client_cert: Vec<u8>,
    client_key: Vec<u8>,
    server_cert: Option<Vec<u8>>,
}

impl AtrClient {
    /// Create a new client for the given host (default ports 6467/6466).
    pub fn new(host: &str) -> Self {
        let (client_cert, client_key) = generate_client_cert().expect("cert generation");
        Self {
            host: host.to_string(),
            pair_port: 6467,
            control_port: 6466,
            client_cert,
            client_key,
            server_cert: None,
        }
    }

    /// Override the pairing port (for testing against a mock).
    pub fn with_pair_port(mut self, port: u16) -> Self {
        self.pair_port = port;
        self
    }

    /// Override the control port (for testing against a mock).
    pub fn with_control_port(mut self, port: u16) -> Self {
        self.control_port = port;
        self
    }

    /// Run the Polo pairing handshake.
    ///
    /// `pairing_code` is the 6-hex code the device displays on screen.
    /// Returns the device identity (name + MAC from the server cert).
    pub fn pair(&mut self, client_name: &str, pairing_code: &str) -> Result<DeviceIdentity> {
        let addr = format!("{}:{}", self.host, self.pair_port);
        let stream = TcpStream::connect(&addr).map_err(AtrError::Tls)?;
        stream.set_nodelay(true).ok();

        let tls_config = build_tls_config(self.client_cert.clone(), self.client_key.clone())?;
        let server_name = ServerName::try_from(self.host.clone())
            .map_err(|e| AtrError::PairingRejected(format!("bad server name: {e}")))?;

        let conn = ClientConnection::new(tls_config, server_name).map_err(AtrError::Rustls)?;
        let mut tls = StreamOwned::new(conn, stream);

        // Extract the server cert from the TLS session.
        let server_cert = tls
            .conn
            .peer_certificates()
            .and_then(|certs| certs.first())
            .cloned()
            .ok_or_else(|| AtrError::PairingRejected("no server cert".into()))?;
        let server_cert_der = server_cert.as_ref().to_vec();
        self.server_cert = Some(server_cert_der.clone());

        // Parse the device identity from the server cert.
        let identity = parse_device_identity(&server_cert_der)?;

        // Run the pairing handshake (blocking IO over the TLS stream).
        run_handshake_blocking(
            &mut tls,
            &self.client_cert,
            &server_cert_der,
            client_name,
            pairing_code,
        )?;

        Ok(identity)
    }

    /// Connect to the control port (6466) and return a control session.
    pub fn connect_control(
        &self,
    ) -> Result<ControlSession<StreamOwned<ClientConnection, TcpStream>>> {
        let addr = format!("{}:{}", self.host, self.control_port);
        let stream = TcpStream::connect(&addr).map_err(AtrError::Tls)?;
        stream.set_nodelay(true).ok();

        let tls_config = build_tls_config(self.client_cert.clone(), self.client_key.clone())?;
        let server_name = ServerName::try_from(self.host.clone())
            .map_err(|e| AtrError::PairingRejected(format!("bad server name: {e}")))?;

        let conn = ClientConnection::new(tls_config, server_name).map_err(AtrError::Rustls)?;
        let tls = StreamOwned::new(conn, stream);

        Ok(ControlSession::new(tls))
    }
}

/// Run the Polo pairing handshake over a TLS stream (blocking).
fn run_handshake_blocking<S>(
    mut tls: S,
    client_cert_der: &[u8],
    server_cert_der: &[u8],
    client_name: &str,
    pairing_code: &str,
) -> Result<()>
where
    S: Read + Write,
{
    use crate::pairing::encode_varint_frame;

    // Step 1: send pairing request.
    let request = OuterMessage {
        protocol_version: 2,
        status: outer_message::Status::Ok as i32,
        pairing_request: Some(PairingRequest {
            service_name: "atvremote".into(),
            client_name: Some(client_name.into()),
        }),
        ..Default::default()
    };
    tls.write_all(&encode_varint_frame(&request))
        .map_err(AtrError::Tls)?;
    tls.flush().map_err(AtrError::Tls)?;

    // Step 2: read pairing_request_ack.
    let ack_msg = read_outer(&mut tls)?;
    if ack_msg.status != outer_message::Status::Ok as i32 {
        return Err(AtrError::PairingRejected(format!(
            "device returned status {}",
            ack_msg.status
        )));
    }
    let _ack: PairingRequestAck = ack_msg
        .pairing_request_ack
        .ok_or_else(|| AtrError::PairingRejected("expected pairing_request_ack".into()))?;

    // Step 3: send options.
    let options_msg = OuterMessage {
        protocol_version: 2,
        status: outer_message::Status::Ok as i32,
        options: Some(Options {
            preferred_role: Some(options::RoleType::Input as i32),
            input_encodings: vec![options::Encoding {
                r#type: options::encoding::EncodingType::Hexadecimal as i32,
                symbol_length: 6,
            }],
            ..Default::default()
        }),
        ..Default::default()
    };
    tls.write_all(&encode_varint_frame(&options_msg))
        .map_err(AtrError::Tls)?;
    tls.flush().map_err(AtrError::Tls)?;

    // Step 4: read device options.
    let device_options = read_outer(&mut tls)?;
    if device_options.status != outer_message::Status::Ok as i32 {
        return Err(AtrError::PairingRejected(format!(
            "device returned status {}",
            device_options.status
        )));
    }
    let _ = device_options
        .options
        .ok_or_else(|| AtrError::PairingRejected("expected options".into()))?;

    // Step 5: send configuration.
    let config_msg = OuterMessage {
        protocol_version: 2,
        status: outer_message::Status::Ok as i32,
        configuration: Some(Configuration {
            encoding: options::Encoding {
                r#type: options::encoding::EncodingType::Hexadecimal as i32,
                symbol_length: 6,
            },
            client_role: options::RoleType::Input as i32,
        }),
        ..Default::default()
    };
    tls.write_all(&encode_varint_frame(&config_msg))
        .map_err(AtrError::Tls)?;
    tls.flush().map_err(AtrError::Tls)?;

    // Step 6: read configuration_ack (device shows pairing code on screen).
    let config_ack = read_outer(&mut tls)?;
    if config_ack.status != outer_message::Status::Ok as i32 {
        return Err(AtrError::PairingRejected(format!(
            "device returned status {}",
            config_ack.status
        )));
    }
    let _ = config_ack
        .configuration_ack
        .ok_or_else(|| AtrError::PairingRejected("expected configuration_ack".into()))?;

    // Step 7: compute the secret and send it.
    let secret = compute_secret(client_cert_der, server_cert_der, pairing_code)?;
    let secret_msg = OuterMessage {
        protocol_version: 2,
        status: outer_message::Status::Ok as i32,
        secret: Some(crate::proto::polo::Secret { secret }),
        ..Default::default()
    };
    tls.write_all(&encode_varint_frame(&secret_msg))
        .map_err(AtrError::Tls)?;
    tls.flush().map_err(AtrError::Tls)?;

    // Step 8: read secret_ack.
    let secret_ack = read_outer(&mut tls)?;
    if secret_ack.status != outer_message::Status::Ok as i32 {
        return Err(AtrError::PairingRejected(format!(
            "pairing rejected: status {}",
            secret_ack.status
        )));
    }
    let _ = secret_ack
        .secret_ack
        .ok_or_else(|| AtrError::PairingRejected("expected secret_ack".into()))?;

    Ok(())
}

/// Compute the Polo pairing secret.
fn compute_secret(
    client_cert_der: &[u8],
    server_cert_der: &[u8],
    pairing_code: &str,
) -> Result<Vec<u8>> {
    let (client_mod, client_exp) = crate::pairing::cert_modulus_exponent(client_cert_der)?;
    let (server_mod, server_exp) = crate::pairing::cert_modulus_exponent(server_cert_der)?;

    let salt = &pairing_code[2..];
    let salt_bytes =
        hex::decode(salt).map_err(|e| AtrError::PairingRejected(format!("bad salt: {e}")))?;

    let mut hasher = Sha256::new();
    hasher.update(&client_mod);
    hasher.update([0x00]);
    hasher.update(&client_exp);
    hasher.update([0x00]);
    hasher.update(&server_mod);
    hasher.update([0x00]);
    hasher.update(&server_exp);
    hasher.update(&salt_bytes);
    let hash = hasher.finalize();

    // Validate: first byte must equal pairing_code[0:2] hex.
    let expected_first = u8::from_str_radix(&pairing_code[0..2], 16)
        .map_err(|e| AtrError::PairingRejected(format!("bad code: {e}")))?;
    if hash[0] != expected_first {
        return Err(AtrError::PairingRejected(
            "pairing code does not match computed secret".into(),
        ));
    }

    Ok(hash.to_vec())
}

/// Read one varint-framed `OuterMessage` from the stream.
fn read_outer<S: Read>(stream: &mut S) -> Result<OuterMessage> {
    let mut len_buf = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        stream.read_exact(&mut byte).map_err(AtrError::Tls)?;
        len_buf.push(byte[0]);
        if byte[0] & 0x80 == 0 {
            break;
        }
    }
    let mut len: u64 = 0;
    let mut shift = 0;
    for &b in &len_buf {
        len |= ((b & 0x7F) as u64) << shift;
        if b & 0x80 == 0 {
            break;
        }
        shift += 7;
    }
    let mut msg_buf = vec![0u8; len as usize];
    stream.read_exact(&mut msg_buf).map_err(AtrError::Tls)?;
    OuterMessage::decode(&msg_buf[..]).map_err(AtrError::Decode)
}
