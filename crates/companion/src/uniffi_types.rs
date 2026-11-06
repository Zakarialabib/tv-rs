//! UniFFI export definitions and bridge types for Swift and Kotlin.
//!
//! Provides C-ABI compatible data structures, callbacks, and functions
//! that bind directly to Swift (via Swift Package Manager) and Kotlin (via JNA/UniFFI).

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use tv_rs_provision::multicast::MobilePlatform;

use crate::ceremony::PairingCeremony;
use crate::control::CompanionInput;
use crate::discovery::{DiscoveredDisplay, DiscoveryGate};
use crate::error::CompanionError;
use crate::hostd::HostdClient;
use crate::remote::RemoteKey;

/// FFI exported display descriptor for Swift/Kotlin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FfiDisplayInfo {
    /// Display ID.
    pub id: String,
    /// Friendly name.
    pub name: String,
    /// Host IP.
    pub host: String,
    /// Port.
    pub port: u16,
}

impl From<DiscoveredDisplay> for FfiDisplayInfo {
    fn from(d: DiscoveredDisplay) -> Self {
        Self {
            id: d.id,
            name: d.name,
            host: d.ip,
            port: d.port,
        }
    }
}

/// Swift/Kotlin compatible Mobile Companion Engine.
#[derive(Debug, Clone)]
pub struct CompanionEngine {
    platform: MobilePlatform,
    discovery_gate: DiscoveryGate,
    ceremony: PairingCeremony,
    hostd_client: Option<HostdClient>,
}

impl CompanionEngine {
    /// Platform this companion engine targets.
    pub fn platform(&self) -> MobilePlatform {
        self.platform
    }

    /// Create engine for specified mobile platform and client identifier.
    pub fn new(platform: MobilePlatform, client_id: impl Into<String>) -> Self {
        let gate = DiscoveryGate::new(platform);
        let ceremony = PairingCeremony::new(client_id);
        Self {
            platform,
            discovery_gate: gate,
            ceremony,
            hostd_client: None,
        }
    }

    /// Report iOS multicast entitlement status.
    pub fn set_ios_multicast_entitlement(&mut self, granted: bool) {
        self.discovery_gate = self.discovery_gate.clone().with_ios_entitlement(granted);
    }

    /// Report Android multicast lock acquisition status.
    pub fn set_android_multicast_lock(&mut self, acquired: bool) {
        self.discovery_gate = self.discovery_gate.clone().with_android_lock(acquired);
    }

    /// Scan displays over local network with entitlement / lock gating.
    pub fn discover_displays(
        &self,
        known_hosts: &[String],
    ) -> Result<Vec<FfiDisplayInfo>, CompanionError> {
        let displays = self.discovery_gate.discover(known_hosts)?;
        Ok(displays.into_iter().map(Into::into).collect())
    }

    /// Handle QR scan payload handover (URI format) and return 6-digit SAS.
    pub fn ingest_qr_uri(&mut self, uri: &str) -> Result<String, CompanionError> {
        let payload = tv_rs_provision::qr::QrPairingPayload::from_uri(uri)
            .map_err(|e| CompanionError::CeremonyFailed(e.to_string()))?;
        self.ceremony.ingest_qr_payload(payload)
    }

    /// Verify 6-digit SAS confirmation code and initialize hostd client.
    pub fn confirm_sas(
        &mut self,
        sas_code: &str,
        host: &str,
        port: u16,
    ) -> Result<String, CompanionError> {
        let token = self.ceremony.confirm_sas(sas_code)?;
        let mut client = HostdClient::new(host, port);
        client.set_session_token(&token);
        self.hostd_client = Some(client);
        Ok(token)
    }

    /// Send keypress to display.
    pub fn send_key(&mut self, key: RemoteKey) -> Result<(), CompanionError> {
        let client = self
            .hostd_client
            .as_mut()
            .ok_or_else(|| CompanionError::Hostd("Hostd client not paired".into()))?;
        client.send_key(key)
    }

    /// Send power command to display.
    pub fn send_power(&mut self, power_on: bool) -> Result<(), CompanionError> {
        let client = self
            .hostd_client
            .as_mut()
            .ok_or_else(|| CompanionError::Hostd("Hostd client not paired".into()))?;
        client.send_power(power_on)
    }

    /// Switch input source.
    pub fn send_input(&mut self, input: CompanionInput) -> Result<(), CompanionError> {
        let client = self
            .hostd_client
            .as_mut()
            .ok_or_else(|| CompanionError::Hostd("Hostd client not paired".into()))?;
        client.send_input(input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_companion_engine_uniffi_flow() {
        let mut engine = CompanionEngine::new(MobilePlatform::Ios, "swift-client-1");
        engine.set_ios_multicast_entitlement(true);

        let displays = engine.discover_displays(&[]).expect("discover");
        assert!(!displays.is_empty());

        let uri = "tvrs-companion://pair?id=screen-5&name=Lobby&ips=192.168.1.50&port=8080&token=xyz789&exp=1900000000";
        let sas = engine.ingest_qr_uri(uri).expect("ingest qr");
        assert_eq!(sas.len(), 6);

        engine
            .confirm_sas(&sas, "192.168.1.50", 8080)
            .expect("confirm sas");
        engine.send_key(RemoteKey::Home).expect("send key");
        engine.send_power(true).expect("send power");
        engine
            .send_input(CompanionInput::Hdmi(1))
            .expect("send input");
    }
}
