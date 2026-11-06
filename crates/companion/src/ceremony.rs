//! QR and SAS pairing ceremony.
//!
//! Handshake workflow:
//! 1. Companion scans QR code (or receives handover) yielding [`tv_rs_provision::qr::QrPairingPayload`].
//! 2. Companion derives the 6-digit Short Authentication String (SAS) matching client identifier.
//! 3. Companion verifies SAS with user on display (or confirms auto-match).
//! 4. Paired session token is established for authenticated control commands.

use alloc::format;
use alloc::string::String;
use tv_rs_provision::qr::QrPairingPayload;

use crate::error::{CompanionError, Result};

/// State of the authentication ceremony.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CeremonyState {
    /// Initial scan state.
    Initial,
    /// SAS generated and awaiting user confirmation.
    AwaitingUserVerification {
        /// Generated 6-digit SAS code.
        sas_code: String,
        /// Ephemeral auth token.
        auth_token: String,
    },
    /// Ceremony successfully completed.
    Paired {
        /// Active session token for control requests.
        session_token: String,
    },
}

/// Dynamic pairing ceremony coordinator.
#[derive(Debug, Clone)]
pub struct PairingCeremony {
    client_id: String,
    payload: Option<QrPairingPayload>,
    state: CeremonyState,
}

impl PairingCeremony {
    /// Create a new pairing ceremony coordinator for a client device (e.g. "iphone-user-123").
    pub fn new(client_id: impl Into<String>) -> Self {
        Self {
            client_id: client_id.into(),
            payload: None,
            state: CeremonyState::Initial,
        }
    }

    /// Client identifier.
    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    /// Current ceremony state.
    pub fn state(&self) -> &CeremonyState {
        &self.state
    }

    /// Ingest scanned QR payload and transition to `AwaitingUserVerification`.
    pub fn ingest_qr_payload(&mut self, payload: QrPairingPayload) -> Result<String> {
        let sas_code = payload.compute_sas(&self.client_id);
        self.state = CeremonyState::AwaitingUserVerification {
            sas_code: sas_code.clone(),
            auth_token: payload.auth_token.clone(),
        };
        self.payload = Some(payload);
        Ok(sas_code)
    }

    /// Confirm SAS verification code shown on screen matches.
    pub fn confirm_sas(&mut self, user_entered_code: &str) -> Result<String> {
        match &self.state {
            CeremonyState::AwaitingUserVerification {
                sas_code,
                auth_token,
            } => {
                if sas_code != user_entered_code {
                    return Err(CompanionError::CeremonyFailed(format!(
                        "SAS mismatch: expected {sas_code}, user entered {user_entered_code}"
                    )));
                }

                let session_token = format!("{}:{}", self.client_id, auth_token);
                self.state = CeremonyState::Paired {
                    session_token: session_token.clone(),
                };
                Ok(session_token)
            }
            _ => Err(CompanionError::CeremonyFailed(
                "Cannot confirm SAS: not in AwaitingUserVerification state".into(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pairing_ceremony_full_flow() {
        let mut ceremony = PairingCeremony::new("user-phone-1");
        assert_eq!(*ceremony.state(), CeremonyState::Initial);

        let payload = QrPairingPayload {
            display_id: "screen-1".into(),
            display_name: "Front Window".into(),
            ip_addresses: alloc::vec!["192.168.1.10".into()],
            port: 8080,
            auth_token: "sec_tok_xyz".into(),
            expires_at: 1800000000,
        };

        let sas = ceremony
            .ingest_qr_payload(payload)
            .expect("Ingest QR payload");
        assert_eq!(sas.len(), 6);

        // Fail with wrong SAS
        let err = ceremony.confirm_sas("000000");
        assert!(err.is_err());

        // Succeed with correct SAS
        let session = ceremony.confirm_sas(&sas).expect("SAS match");
        assert!(session.contains("user-phone-1"));
        assert!(matches!(ceremony.state(), CeremonyState::Paired { .. }));
    }
}
