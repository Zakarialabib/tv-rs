//! QR pairing handover structures and validation.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use sha2::{Digest, Sha256};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::error::{ProvisionError, Result};

/// URI scheme used for tv-rs companion pairing.
pub const COMPANION_PAIRING_URI_SCHEME: &str = "tvrs-companion";

/// QR pairing handover packet containing display endpoint information.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct QrPairingPayload {
    /// Unique display identifier.
    pub display_id: String,
    /// Friendly name or model.
    pub display_name: String,
    /// IP addresses available on the display/host.
    pub ip_addresses: Vec<String>,
    /// Port of tv-rs-hostd or control service.
    pub port: u16,
    /// Ephemeral public key / token for SAS calculation.
    pub auth_token: String,
    /// Expiration timestamp in seconds since epoch.
    pub expires_at: u64,
}

impl QrPairingPayload {
    /// Create a new QR pairing payload.
    pub fn new(
        display_id: impl Into<String>,
        display_name: impl Into<String>,
        ip_addresses: Vec<String>,
        port: u16,
        auth_token: impl Into<String>,
        expires_at: u64,
    ) -> Self {
        Self {
            display_id: display_id.into(),
            display_name: display_name.into(),
            ip_addresses,
            port,
            auth_token: auth_token.into(),
            expires_at,
        }
    }

    /// Compute the Short Authentication String (SAS) code matching the pairing token and client identifier.
    ///
    /// Computes SHA256(display_id + auth_token + client_id) and extracts a 6-digit verification code.
    pub fn compute_sas(&self, client_id: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.display_id.as_bytes());
        hasher.update(self.auth_token.as_bytes());
        hasher.update(client_id.as_bytes());
        let hash = hasher.finalize();

        // 6-digit decimal SAS
        let val = ((hash[0] as u32) << 24)
            | ((hash[1] as u32) << 16)
            | ((hash[2] as u32) << 8)
            | (hash[3] as u32);
        format!("{:06}", val % 1_000_000)
    }

    /// Convert to URI string: `tvrs-companion://pair?id=...&name=...&ips=...&port=...&token=...&exp=...`
    pub fn to_uri(&self) -> String {
        let ips = self.ip_addresses.join(",");
        format!(
            "{}://pair?id={}&name={}&ips={}&port={}&token={}&exp={}",
            COMPANION_PAIRING_URI_SCHEME,
            self.display_id,
            self.display_name,
            ips,
            self.port,
            self.auth_token,
            self.expires_at
        )
    }

    /// Parse from URI string.
    pub fn from_uri(uri: &str) -> Result<Self> {
        let prefix = format!("{COMPANION_PAIRING_URI_SCHEME}://pair?");
        if !uri.starts_with(&prefix) {
            return Err(ProvisionError::QrPayload(
                "Invalid URI scheme or endpoint".into(),
            ));
        }

        let query = &uri[prefix.len()..];
        let mut display_id = None;
        let mut display_name = None;
        let mut ip_addresses = Vec::new();
        let mut port = None;
        let mut auth_token = None;
        let mut expires_at = None;

        for pair in query.split('&') {
            let mut parts = pair.splitn(2, '=');
            let key = parts.next().unwrap_or("");
            let val = parts.next().unwrap_or("");

            match key {
                "id" => display_id = Some(val.into()),
                "name" => display_name = Some(val.into()),
                "ips" => {
                    ip_addresses = val
                        .split(',')
                        .filter(|s| !s.is_empty())
                        .map(Into::into)
                        .collect();
                }
                "port" => {
                    port = val.parse::<u16>().ok();
                }
                "token" => auth_token = Some(val.into()),
                "exp" => expires_at = val.parse::<u64>().ok(),
                _ => {}
            }
        }

        let display_id =
            display_id.ok_or_else(|| ProvisionError::QrPayload("Missing id".into()))?;
        let display_name = display_name.unwrap_or_else(|| "Display".into());
        let port = port.ok_or_else(|| ProvisionError::QrPayload("Missing port".into()))?;
        let auth_token =
            auth_token.ok_or_else(|| ProvisionError::QrPayload("Missing token".into()))?;
        let expires_at = expires_at.unwrap_or(0);

        Ok(Self {
            display_id,
            display_name,
            ip_addresses,
            port,
            auth_token,
            expires_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qr_payload_uri_roundtrip() {
        let payload = QrPairingPayload {
            display_id: "disp-101".into(),
            display_name: "Lobby OLED".into(),
            ip_addresses: alloc::vec!["192.168.1.50".into(), "10.0.0.50".into()],
            port: 8080,
            auth_token: "tok_sec_123456789".into(),
            expires_at: 1700000000,
        };

        let uri = payload.to_uri();
        assert!(uri.starts_with("tvrs-companion://pair?"));

        let decoded = QrPairingPayload::from_uri(&uri).expect("Parse URI");
        assert_eq!(decoded, payload);
    }

    #[test]
    fn test_compute_sas_determinism() {
        let payload = QrPairingPayload {
            display_id: "disp-101".into(),
            display_name: "Lobby OLED".into(),
            ip_addresses: alloc::vec!["192.168.1.50".into()],
            port: 8080,
            auth_token: "tok_sec_123456789".into(),
            expires_at: 1700000000,
        };

        let sas1 = payload.compute_sas("client-iphone");
        let sas2 = payload.compute_sas("client-iphone");
        assert_eq!(sas1, sas2);
        assert_eq!(sas1.len(), 6);

        let sas_different = payload.compute_sas("client-android");
        assert_ne!(sas1, sas_different);
    }
}
