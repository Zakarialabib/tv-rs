//! The shared `DeviceHandle` adapter for an MDC display.

use std::time::{Duration, Instant};

use async_trait::async_trait;
use tv_rs_core::{
    BrightnessControl, Capability, CapabilitySet, CommandId, DeviceDescriptor, DeviceHandle,
    DeviceId, Input, InputSelect, PowerControl, PowerState, Probe, ProtocolError, Rebootable,
    TvError, VolumeControl,
};

use crate::client::MdcClient;
use crate::payload::Status;
use crate::types::{InputSource, MdcPower};

/// An MDC display exposed through the vendor-neutral device model.
///
/// The underlying [`MdcClient`] remains available for raw frames; the shared traits
/// cover power, input, volume, brightness, reboot and a control-channel probe.
pub struct MdcDevice {
    client: MdcClient,
    descriptor: DeviceDescriptor,
}

impl MdcDevice {
    /// Wraps a client with the descriptor discovery or configuration supplied.
    #[must_use]
    pub fn new(client: MdcClient, descriptor: DeviceDescriptor) -> Self {
        Self { client, descriptor }
    }

    /// The underlying MDC client.
    #[must_use]
    pub fn client(&self) -> &MdcClient {
        &self.client
    }

    /// Reads the full MDC status.
    ///
    /// # Errors
    ///
    /// Returns the MDC transport, protocol or `NAK` error mapped into [`TvError`].
    pub async fn status(&self) -> Result<Status, TvError> {
        Ok(self.client.status().await?)
    }

    /// Reads brightness. `None` means the panel does not report it.
    ///
    /// # Errors
    ///
    /// Returns the MDC error mapped into [`TvError`].
    pub async fn brightness(&self) -> Result<Option<u8>, TvError> {
        Ok(self.client.brightness().await?)
    }

    /// Sets brightness, 0-100.
    ///
    /// # Errors
    ///
    /// Returns [`TvError::Protocol`] for a value above 100, or the mapped MDC
    /// transport, protocol or `NAK` error.
    pub async fn set_brightness(&self, level: u8) -> Result<(), TvError> {
        Ok(self.client.set_brightness(level).await?)
    }
}

#[async_trait]
impl PowerControl for MdcDevice {
    async fn power_state(&self) -> Result<PowerState, TvError> {
        Ok(match self.client.power().await? {
            MdcPower::On => PowerState::On,
            MdcPower::Off => PowerState::Off,
            MdcPower::Reboot => PowerState::Unknown,
        })
    }

    async fn wake(&self) -> Result<(), TvError> {
        // The MDC specification requires Wake-on-LAN when the panel is off; a
        // TCP frame is not a wake transport. Advertising it would be a lie.
        Err(TvError::Unsupported(Capability::Wake))
    }

    async fn sleep(&self) -> Result<(), TvError> {
        Ok(self.client.power_off().await?)
    }
}

#[async_trait]
impl InputSelect for MdcDevice {
    async fn active_input(&self) -> Result<Option<Input>, TvError> {
        Ok(self
            .client
            .active_input()
            .await?
            .map(InputSource::to_core_input))
    }

    async fn select_input(&self, input: Input, _cmd: CommandId) -> Result<(), TvError> {
        let source = InputSource::from_core_input(&input).ok_or_else(|| {
            TvError::Protocol(ProtocolError::Unexpected(format!(
                "MDC has no input code for {input}"
            )))
        })?;
        Ok(self.client.set_input(source).await?)
    }
}

#[async_trait]
impl VolumeControl for MdcDevice {
    async fn volume(&self) -> Result<Option<u8>, TvError> {
        Ok(self.client.volume().await?)
    }

    async fn set_volume(&self, level: u8, _cmd: CommandId) -> Result<(), TvError> {
        Ok(self.client.set_volume(level).await?)
    }

    async fn set_mute(&self, muted: bool, _cmd: CommandId) -> Result<(), TvError> {
        Ok(self.client.set_mute(muted).await?)
    }
}

#[async_trait]
impl BrightnessControl for MdcDevice {
    async fn brightness(&self) -> Result<Option<u8>, TvError> {
        Ok(self.client.brightness().await?)
    }

    async fn set_brightness(&self, level: u8, _cmd: CommandId) -> Result<(), TvError> {
        Ok(self.client.set_brightness(level).await?)
    }
}

#[async_trait]
impl Rebootable for MdcDevice {
    async fn reboot(&self, _cmd: CommandId) -> Result<(), TvError> {
        Ok(self.client.reboot().await?)
    }
}

#[async_trait]
impl Probe for MdcDevice {
    async fn ping(&self) -> Result<Duration, TvError> {
        let started = Instant::now();
        self.client.status().await?;
        Ok(started.elapsed())
    }
}

impl DeviceHandle for MdcDevice {
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
            .with(Capability::Brightness)
            .with(Capability::Reboot)
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

    fn as_brightness(&self) -> Option<&dyn BrightnessControl> {
        self.capabilities()
            .contains(Capability::Brightness)
            .then_some(self as &dyn BrightnessControl)
    }

    fn as_reboot(&self) -> Option<&dyn Rebootable> {
        self.capabilities()
            .contains(Capability::Reboot)
            .then_some(self as &dyn Rebootable)
    }

    fn as_probe(&self) -> Option<&dyn Probe> {
        Some(self)
    }
}

#[cfg(test)]
mod tests {
    use tv_rs_core::conformance;
    use tv_rs_core::{DeviceDescriptor, DeviceId, DeviceOs};

    use super::MdcDevice;
    use crate::client::MdcClient;
    use crate::types::DisplayId;

    #[tokio::test]
    async fn the_handle_satisfies_the_shared_conformance_suite_without_hardware() {
        let client =
            MdcClient::new("127.0.0.1").display(DisplayId::new(1).expect("valid display id"));
        let descriptor = DeviceDescriptor::new(
            DeviceId::new("mdc-test-1").expect("valid id"),
            DeviceOs::Unknown,
            "127.0.0.1:1515",
        );
        let device = MdcDevice::new(client, descriptor);
        let violations = conformance::run(&device).await;
        assert!(violations.is_empty(), "{violations:#?}");
    }
}
