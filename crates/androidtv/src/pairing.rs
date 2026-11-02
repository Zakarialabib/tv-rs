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
use crate::proto::polo::OuterMessage;
use prost::Message;
use sha2::{Digest, Sha256};
use std::io::Read;

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
        .find(|a| a.attr_type() == &x509_parser::oid_registry::OID_X509_COMMON_NAME)
        .and_then(|a| a.as_str().ok())
        .unwrap_or_default()
        .to_string();
    let dn_qualifier = cert
        .subject()
        .iter_attributes()
        .find(|a| a.attr_type() == &x509_parser::oid_registry::OID_X509_DN_QUALIFIER)
        .and_then(|a| a.as_str().ok())
        .unwrap_or_default()
        .to_string();

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
    let spki = cert.public_key();
    let pk = spki
        .parsed()
        .map_err(|e| AtrError::PairingRejected(format!("bad public key: {e}")))?;
    match pk {
        x509_parser::public_key::PublicKey::RSA(rsa) => {
            Ok((rsa.modulus.to_vec(), rsa.exponent.to_vec()))
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
    hasher.update([0x00]);
    hasher.update(&client_exp);
    hasher.update([0x00]);
    hasher.update(&server_mod);
    hasher.update([0x00]);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A pure-Rust RSA key pair handed to rcgen for signing (mirrors
    /// `generate_client_cert`; rcgen's own RSA keygen needs aws-lc-rs).
    fn rsa_key_pair() -> rcgen::KeyPair {
        use rsa::pkcs8::EncodePrivateKey;
        let private_key = {
            let mut rng = rand::thread_rng();
            rsa::RsaPrivateKey::new(&mut rng, 2048).expect("rsa keygen")
        };
        let pkcs8 = private_key.to_pkcs8_der().expect("pkcs8");
        rcgen::KeyPair::from_pkcs8_der_and_sign_algo(
            &pki_types::PrivatePkcs8KeyDer::from(pkcs8.as_bytes()),
            &rcgen::PKCS_RSA_SHA256,
        )
        .expect("key pair from pkcs8")
    }

    /// Build an RSA self-signed cert with a `CN=atvremote/<cn>/<mac>` subject.
    fn rsa_cert(cn: &str) -> Vec<u8> {
        let key_pair = rsa_key_pair();
        let mut params = rcgen::CertificateParams::new(vec![]).expect("params");
        params
            .distinguished_name
            .push(rcgen::DnType::CommonName, cn);
        let cert = params.self_signed(&key_pair).expect("self-sign");
        cert.der().to_vec()
    }

    #[test]
    fn encode_varint_shapes() {
        let mut buf = Vec::new();
        encode_varint(0, &mut buf);
        assert_eq!(buf, vec![0x00]);

        buf.clear();
        encode_varint(127, &mut buf);
        assert_eq!(buf, vec![0x7F]);

        buf.clear();
        encode_varint(128, &mut buf);
        assert_eq!(buf, vec![0x80, 0x01]);

        buf.clear();
        encode_varint(300, &mut buf);
        assert_eq!(buf, vec![0xAC, 0x02]);
    }

    #[test]
    fn varint_round_trip() {
        for value in [0u64, 1, 127, 128, 300, u32::MAX as u64, 1 << 40] {
            let mut buf = Vec::new();
            encode_varint(value, &mut buf);
            let (decoded, consumed) = decode_varint_prefix(&buf).expect("varint decodes");
            assert_eq!(decoded, value as usize);
            assert_eq!(consumed, buf.len());
        }
    }

    #[test]
    fn decode_varint_prefix_incomplete() {
        // 0x80 alone is an incomplete varint (continuation bit set, no follow-up).
        assert_eq!(decode_varint_prefix(&[0x80]), None);
    }

    #[test]
    fn parse_device_identity_shield_style() {
        let cert = rsa_cert("atvremote/darcy/darcy/SHIELD Android TV/AA:BB:CC:DD:EE:FF");
        let identity = parse_device_identity(&cert).expect("identity");
        // The reference client takes common_name_parts[-2]: here that is
        // "SHIELD Android TV" (the human-readable device name), not the
        // client label "darcy" repeated earlier in the CN.
        assert_eq!(identity.name, "SHIELD Android TV");
        assert_eq!(identity.mac, "AA:BB:CC:DD:EE:FF");
    }

    #[test]
    fn parse_device_identity_nexus_player_style() {
        // Nexus Player: name in dnQualifier, CN = atvremote/MAC.
        let key_pair = rsa_key_pair();
        let mut params = rcgen::CertificateParams::new(vec![]).expect("params");
        params.distinguished_name.push(
            rcgen::DnType::CustomDnType(vec![2, 5, 4, 46]),
            "fugu/fugu/Nexus Player",
        );
        params
            .distinguished_name
            .push(rcgen::DnType::CommonName, "atvremote/11:22:33:44:55:66");
        let cert = params.self_signed(&key_pair).expect("self-sign");
        let identity = parse_device_identity(cert.der()).expect("identity");
        assert_eq!(identity.name, "Nexus Player");
        assert_eq!(identity.mac, "11:22:33:44:55:66");
    }

    #[test]
    fn pairing_secret_round_trip() {
        let client = rsa_cert("atvremote/tv-rs");
        let server = rsa_cert("atvremote/panel/panel/Test TV/11:22:33:44:55:66");

        // The secret depends on the code, so compute a hash and then pick a
        // code whose first byte matches it: the protocol's own validation is
        // "hash[0] == code[0:2]". We test compute + validate agreement by
        // brute-forcing the first code byte.
        let mut found = None;
        for hi in 0x00..=0xFFu8 {
            let code = format!("{hi:02X}0000");
            if let Ok(secret) = compute_pairing_secret(&client, &server, &code) {
                if secret[0] == hi {
                    found = Some((code, secret));
                    break;
                }
            }
        }
        let (code, secret) = found.expect("a matching code exists");
        validate_pairing_code(&code, &secret).expect("validate accepts the matching code");

        // A wrong code (different first byte) must be rejected.
        let wrong: String = format!("{:02X}0000", (secret[0] ^ 0xFF) as u32).to_lowercase();
        assert!(validate_pairing_code(&wrong, &secret).is_err());
    }

    #[test]
    fn pairing_secret_rejects_short_codes() {
        let client = rsa_cert("atvremote/tv-rs");
        let server = rsa_cert("atvremote/panel/panel/Test TV/11:22:33:44:55:66");
        assert!(compute_pairing_secret(&client, &server, "12345").is_err());
        assert!(validate_pairing_code("12", &[0u8; 32]).is_err());
    }
}
