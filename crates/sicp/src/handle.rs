//! The erased `DeviceHandle` adapter for a Philips SICP display.

use std::time::{Duration, Instant};

use async_trait::async_trait;
use tv_rs_core::{
    Capability, CapabilitySet, CommandId, DeviceDescriptor, DeviceHandle, DeviceId, Input,
    InputSelect, ObservedField, PanelHealth, PanelHealthProvider, PowerControl, PowerState, Probe,
    ProtocolError, Telemetry, TelemetryProvider, TvError, VolumeControl,
};

use crate::client::SicpClient;
use crate::payload::Status;
use crate::types::{SicpInput, SicpPower};

/// A Philips SICP display exposed through the vendor-neutral device model.
pub struct SicpDevice {
    client: SicpClient,
    descriptor: DeviceDescriptor,
}

impl SicpDevice {
    /// Wraps a client with descriptor metadata.
    #[must_use]
    pub fn new(client: SicpClient, descriptor: DeviceDescriptor) -> Self {
        Self { client, descriptor }
    }

    /// Access the underlying SICP client.
    #[must_use]
    pub fn client(&self) -> &SicpClient {
        &self.client
    }

    /// Reads full display status and telemetry.
    ///
    /// # Errors
    ///
    /// Returns mapped [`TvError`].
    pub async fn status(&self) -> Result<Status, TvError> {
        Ok(self.client.status().await?)
    }
}

#[async_trait]
impl PowerControl for SicpDevice {
    async fn power_state(&self) -> Result<PowerState, TvError> {
        Ok(match self.client.power().await? {
            SicpPower::On => PowerState::On,
            SicpPower::Off => PowerState::Off,
            SicpPower::Standby => PowerState::Standby,
        })
    }

    async fn wake(&self) -> Result<(), TvError> {
        Err(TvError::Unsupported(Capability::Wake))
    }

    async fn sleep(&self) -> Result<(), TvError> {
        Ok(self.client.power_off().await?)
    }
}

#[async_trait]
impl InputSelect for SicpDevice {
    async fn active_input(&self) -> Result<Option<Input>, TvError> {
        Ok(self
            .client
            .active_input()
            .await?
            .map(|inp| inp.to_core_input()))
    }

    async fn select_input(&self, input: Input, _cmd: CommandId) -> Result<(), TvError> {
        let sicp_input = SicpInput::from_core_input(&input).ok_or_else(|| {
            TvError::Protocol(ProtocolError::Unexpected(format!(
                "SICP has no input code for {input}"
            )))
        })?;
        Ok(self.client.set_input(sicp_input).await?)
    }
}

#[async_trait]
impl VolumeControl for SicpDevice {
    async fn volume(&self) -> Result<Option<u8>, TvError> {
        Ok(self.client.volume().await?)
    }

    async fn set_volume(&self, level: u8, _cmd: CommandId) -> Result<(), TvError> {
        Ok(self.client.set_volume(level).await?)
    }

    async fn set_mute(&self, _muted: bool, _cmd: CommandId) -> Result<(), TvError> {
        Err(TvError::Unsupported(Capability::Volume))
    }

    async fn mute(&self) -> Result<Option<bool>, TvError> {
        Ok(None)
    }
}

#[async_trait]
impl PanelHealthProvider for SicpDevice {
    async fn panel_health(&self) -> Result<Option<PanelHealth>, TvError> {
        let st = self.client.status().await?;
        Ok(Some(PanelHealth {
            signal_present: st.signal_present,
            temperature_celsius: Some(i16::from(st.temperature_celsius)),
            lamp_error: st.lamp_error,
        }))
    }
}

#[async_trait]
impl TelemetryProvider for SicpDevice {
    async fn telemetry(&self) -> Result<Telemetry, TvError> {
        let st = self.client.status().await?;
        Ok(Telemetry {
            temperature_celsius: ObservedField::from_read(i16::from(st.temperature_celsius)),
            ..Telemetry::nothing_observed()
        })
    }
}

#[async_trait]
impl Probe for SicpDevice {
    async fn ping(&self) -> Result<Duration, TvError> {
        let started = Instant::now();
        let _ = self.client.power().await?;
        Ok(started.elapsed())
    }
}

impl DeviceHandle for SicpDevice {
    fn id(&self) -> DeviceId {
        self.descriptor.id.clone()
    }

    fn descriptor(&self) -> DeviceDescriptor {
        self.descriptor.clone()
    }

    fn capabilities(&self) -> CapabilitySet {
        CapabilitySet::empty()
            .with(Capability::PowerState)
            .with(Capability::InputSelect)
            .with(Capability::Volume)
            .with(Capability::Telemetry)
            .with(Capability::NoSignalStatus)
    }

    fn as_power(&self) -> Option<&dyn PowerControl> {
        self.capabilities()
            .contains(Capability::PowerState)
            .then_some(self as &dyn PowerControl)
    }

    fn as_key(&self) -> Option<&dyn tv_rs_core::KeyInput> {
        None
    }

    fn as_app(&self) -> Option<&dyn tv_rs_core::AppLaunch> {
        None
    }

    fn as_text(&self) -> Option<&dyn tv_rs_core::TextInput> {
        None
    }

    fn as_input(&self) -> Option<&dyn InputSelect> {
        self.capabilities()
            .contains(Capability::InputSelect)
            .then_some(self as &dyn InputSelect)
    }

    fn as_volume(&self) -> Option<&dyn VolumeControl> {
        self.capabilities()
            .contains(Capability::Volume)
            .then_some(self as &dyn VolumeControl)
    }

    fn as_panel_health(&self) -> Option<&dyn PanelHealthProvider> {
        self.capabilities()
            .contains(Capability::NoSignalStatus)
            .then_some(self as &dyn PanelHealthProvider)
    }

    fn as_telemetry(&self) -> Option<&dyn TelemetryProvider> {
        self.capabilities()
            .contains(Capability::Telemetry)
            .then_some(self as &dyn TelemetryProvider)
    }

    fn as_probe(&self) -> Option<&dyn Probe> {
        Some(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tv_rs_core::{conformance, DeviceDescriptor, DeviceId, DeviceOs};

    use crate::client::SicpClient;
    use crate::types::DisplayId;

    #[tokio::test]
    async fn handle_conformance_check() {
        let client = SicpClient::new("127.0.0.1").display(DisplayId::new(1).unwrap());
        let descriptor = DeviceDescriptor::new(
            DeviceId::new("sicp-test-1").unwrap(),
            DeviceOs::PhilipsSicp,
            "127.0.0.1:5000",
        );
        let device = SicpDevice::new(client, descriptor);
        let violations = conformance::run(&device).await;
        assert!(violations.is_empty(), "{violations:#?}");
    }
}
