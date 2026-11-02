//! Polo pairing handshake (port 6467, mTLS).
//!
//! The pairing flow, from the reference client (`androidtvremote2/pairing.py`):
//!
//! 1. Client connects with TLS (self-signed client cert, server cert accepted
//!    without verification — the client pins it implicitly by using it in the
//!    secret computation).
//! 2. Client sends `OuterMessage{protocol_version=2, status=OK,
//!    pairing_request{client_name, service_name}}`.
//! 3. Device replies `pairing_request_ack{server_name}`.
//! 4. Client sends `options{preferred_role=INPUT,
//!    input_encodings=[{HEXADECIMAL, 6}]}`.
//! 5. Device replies `options{...}`.
//! 6. Client sends `configuration{client_role=INPUT,
//!    encoding={HEXADECIMAL, 6}}`.
//! 7. Device replies `configuration_ack` — the device now shows a 6-hex
//!    pairing code on screen. The user reads it and enters it.
//! 8. Client computes
//!    `SHA-256(client_modulus || 0x0 || client_exponent || 0x0 ||
//!    server_modulus || 0x0 || server_exponent || pairing_code[2:])`
//!    and sends `secret{secret=hash}`.
//! 9. Device replies `secret_ack` — pairing complete.

use crate::error::{AtrError, Result};
use crate::proto::polo::{
    options, outer_message, Configuration, Options, OuterMessage, PairingRequest, PairingRequestAck,
};
use prost::Message;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};

/// A parsed device identity from the server certificate subject.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceIdentity {
    /// Human-readable device name (e.g. "SHIELD Android TV").
    pub name: String,
    /// Device MAC address (e.g. "AA:BB:CC:DD:EE:FF").
    pub mac: String,
}

/// Extract the device name and MAC from an Android TV server certificate.
///
/// NVIDIA SHIELD example: `CN=atvremote/darcy/darcy/SHIELD Android TV/XX:XX:XX:XX:XX:XX`
/// Nexus Player example: `dnQualifier=fugu/fugu/Nexus Player/CN=atvremote/XX:XX:XX:XX:XX:XX`
pub fn parse_device_identity(server_cert_der: &[u8]) -> Result<DeviceIdentity> {
    let (_, cert) = x509_parser::parse_x509_certificate(server_cert_der)
        .map_err(|e| AtrError::PairingRejected(format!("bad server cert: {e}")))?;

    let common_name = cert
        .subject()
        .iter_attributes()
        .find(|a| a.oid() == x509_parser::oid_registry::OID_X509_COMMON_NAME)
        .map(|a| a.value().as_str().unwrap_or("").to_string())
        .unwrap_or_default();
    let dn_qualifier = cert
        .subject()
        .iter_attributes()
        .find(|a| a.oid() == x509_parser::oid_registry::OID_X509_DN_QUALIFIER)
        .map(|a| a.value().as_str().unwrap_or("").to_string())
        .unwrap_or_default();

    let cn_parts: Vec<&str> = common_name.split('/').collect();
    let dn_parts: Vec<&str> = dn_qualifier.split('/').collect();

    let name = if !dn_qualifier.is_empty() {
        dn_parts.last().copied().unwrap_or("").to_string()
    } else if cn_parts.len() > 1 {
        cn_parts[cn_parts.len() - 2].to_string()
    } else {
        common_name.clone()
    };
    let mac = cn_parts.last().copied().unwrap_or("").to_string();

    Ok(DeviceIdentity { name, mac })
}

/// Extract the RSA modulus and exponent from a certificate's public key.
pub fn cert_modulus_exponent(cert_der: &[u8]) -> Result<(Vec<u8>, Vec<u8>)> {
    let (_, cert) = x509_parser::parse_x509_certificate(cert_der)
        .map_err(|e| AtrError::PairingRejected(format!("bad cert: {e}")))?;
    let pubkey = cert.public_key();
    match pubkey {
        x509_parser::public_key::PublicKey::RSA(rsa) => {
            Ok((rsa.modulus.to_bytes_be(), rsa.exponent.to_bytes_be()))
        }
        _ => Err(AtrError::PairingRejected("public key is not RSA".into())),
    }
}

/// Compute the Polo pairing secret.
///
/// `SHA-256(client_modulus || 0x0 || client_exponent || 0x0 ||
/// server_modulus || 0x0 || server_exponent || pairing_code[2:])`
///
/// The first byte of the result must equal `pairing_code[0:2]` hex.
pub fn compute_pairing_secret(
    client_cert_der: &[u8],
    server_cert_der: &[u8],
    pairing_code: &str,
) -> Result<Vec<u8>> {
    if pairing_code.len() != 6 {
        return Err(AtrError::PairingRejected(
            "pairing code must be 6 hex chars".into(),
        ));
    }
    let (client_mod, client_exp) = cert_modulus_exponent(client_cert_der)?;
    let (server_mod, server_exp) = cert_modulus_exponent(server_cert_der)?;

    let salt = &pairing_code[2..];
    let salt_bytes = hex::decode(salt)
        .map_err(|e| AtrError::PairingRejected(format!("bad pairing code salt: {e}")))?;

    let mut hasher = Sha256::new();
    hasher.update(&client_mod);
    hasher.update(&[0x00]);
    hasher.update(&client_exp);
    hasher.update(&[0x00]);
    hasher.update(&server_mod);
    hasher.update(&[0x00]);
    hasher.update(&server_exp);
    hasher.update(&salt_bytes);
    let hash = hasher.finalize();

    Ok(hash.to_vec())
}

/// Validate a pairing code against the computed secret.
///
/// Returns `Ok(())` if the first byte of the hash equals `pairing_code[0:2]`.
pub fn validate_pairing_code(pairing_code: &str, secret: &[u8]) -> Result<()> {
    if pairing_code.len() != 6 {
        return Err(AtrError::PairingRejected(
            "pairing code must be 6 hex chars".into(),
        ));
    }
    let expected_first_byte = u8::from_str_radix(&pairing_code[0..2], 16)
        .map_err(|e| AtrError::PairingRejected(format!("bad pairing code: {e}")))?;
    if secret.first().copied() != Some(expected_first_byte) {
        return Err(AtrError::PairingRejected(
            "pairing code does not match computed secret".into(),
        ));
    }
    Ok(())
}

/// Encode a protobuf message with a varint length prefix.
pub fn encode_varint(mut value: u64, buf: &mut Vec<u8>) {
    loop {
        let byte = (value & 0x7F) as u8;
        value >>= 7;
        if value == 0 {
            buf.push(byte);
            break;
        }
        buf.push(byte | 0x80);
    }
}

/// Encode a protobuf message with a varint length prefix.
pub fn encode_varint_frame(msg: &impl Message) -> Vec<u8> {
    let mut buf = Vec::new();
    let len = msg.encoded_len();
    encode_varint(len as u64, &mut buf);
    msg.encode(&mut buf)
        .expect("prost encode into Vec cannot fail");
    buf
}

/// Decode a varint length prefix from a byte stream.
///
/// Returns `(message_len, bytes_consumed)` or `None` if the varint is
/// incomplete.
pub fn decode_varint_prefix(buf: &[u8]) -> Option<(usize, usize)> {
    let mut len: u64 = 0;
    let mut shift = 0;
    for (i, &b) in buf.iter().enumerate() {
        len |= ((b & 0x7F) as u64) << shift;
        if b & 0x80 == 0 {
            return Some((len as usize, i + 1));
        }
        shift += 7;
        if shift > 63 {
            return None;
        }
    }
    None
}

/// Read one varint-framed `OuterMessage` from the stream.
pub fn read_outer_message<S: Read>(stream: &mut S) -> Result<OuterMessage> {
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
