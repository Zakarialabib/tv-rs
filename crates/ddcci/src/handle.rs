//! Vendor-neutral `DeviceHandle` implementation for DDC/CI attached monitors.

use std::sync::Arc;

use async_trait::async_trait;
use tv_rs_core::{
    AppLaunch, BrightnessControl, Capability, CapabilitySet, CommandId, DeviceDescriptor,
    DeviceHandle, DeviceId, Input, InputSelect, KeyInput, PowerControl, PowerState, TextInput,
    TvError, VolumeControl,
};

use crate::backend::{DdcCiBackend, MockBackend};
use crate::vcp::{
    input_to_vcp_value, power_state_to_vcp_value, vcp_value_to_input, vcp_value_to_power_state,
    VcpOpcode,
};

/// A physically attached monitor controlled over DDC/CI, implementing [`DeviceHandle`].
pub struct DdcCiDeviceHandle {
    backend: Arc<dyn DdcCiBackend>,
    descriptor: DeviceDescriptor,
}

impl DdcCiDeviceHandle {
    /// Creates a new [`DdcCiDeviceHandle`] with a backend transport and device descriptor.
    #[must_use]
    pub fn new(backend: Arc<dyn DdcCiBackend>, descriptor: DeviceDescriptor) -> Self {
        Self {
            backend,
            descriptor,
        }
    }

    /// Creates a mock [`DdcCiDeviceHandle`] for testing or headless environments.
    #[must_use]
    pub fn mock(descriptor: DeviceDescriptor) -> Self {
        Self::new(Arc::new(MockBackend::new()), descriptor)
    }

    /// Returns a reference to the underlying [`DdcCiBackend`].
    #[must_use]
    pub fn backend(&self) -> &dyn DdcCiBackend {
        self.backend.as_ref()
    }
}

#[async_trait]
impl PowerControl for DdcCiDeviceHandle {
    async fn power_state(&self) -> Result<PowerState, TvError> {
        let (cur, _) = self.backend.get_vcp(VcpOpcode::PowerState).await?;
        Ok(vcp_value_to_power_state(cur))
    }

    async fn wake(&self) -> Result<(), TvError> {
        let vcp_val = power_state_to_vcp_value(PowerState::On);
        self.backend.set_vcp(VcpOpcode::PowerState, vcp_val).await
    }

    async fn sleep(&self) -> Result<(), TvError> {
        let vcp_val = power_state_to_vcp_value(PowerState::Off);
        self.backend.set_vcp(VcpOpcode::PowerState, vcp_val).await
    }
}

#[async_trait]
impl InputSelect for DdcCiDeviceHandle {
    async fn active_input(&self) -> Result<Option<Input>, TvError> {
        let (cur, _) = self.backend.get_vcp(VcpOpcode::InputSelect).await?;
        Ok(Some(vcp_value_to_input(cur)))
    }

    async fn select_input(&self, input: Input, _cmd: CommandId) -> Result<(), TvError> {
        let vcp_val = input_to_vcp_value(&input)?;
        self.backend.set_vcp(VcpOpcode::InputSelect, vcp_val).await
    }
}

#[async_trait]
impl BrightnessControl for DdcCiDeviceHandle {
    async fn brightness(&self) -> Result<Option<u8>, TvError> {
        let (cur, _) = self.backend.get_vcp(VcpOpcode::Brightness).await?;
        u8::try_from(cur)
            .map(Some)
            .map_err(|_| TvError::protocol(format!("Brightness VCP value {cur} out of u8 range")))
    }

    async fn set_brightness(&self, level: u8, _cmd: CommandId) -> Result<(), TvError> {
        if level > 100 {
            return Err(TvError::protocol(format!(
                "Brightness level {level} exceeds maximum of 100"
            )));
        }
        self.backend
            .set_vcp(VcpOpcode::Brightness, u16::from(level))
            .await
    }
}

impl DeviceHandle for DdcCiDeviceHandle {
    fn id(&self) -> DeviceId {
        self.descriptor.id.clone()
    }

    fn descriptor(&self) -> DeviceDescriptor {
        self.descriptor.clone()
    }

    fn capabilities(&self) -> CapabilitySet {
        CapabilitySet::empty()
            .with(Capability::PowerState)
            .with(Capability::Wake)
            .with(Capability::InputSelect)
            .with(Capability::Brightness)
    }

    fn as_power(&self) -> Option<&dyn PowerControl> {
        Some(self)
    }

    fn as_key(&self) -> Option<&dyn KeyInput> {
        None
    }

    fn as_app(&self) -> Option<&dyn AppLaunch> {
        None
    }

    fn as_text(&self) -> Option<&dyn TextInput> {
        None
    }

    fn as_input(&self) -> Option<&dyn InputSelect> {
        Some(self)
    }

    fn as_volume(&self) -> Option<&dyn VolumeControl> {
        None
    }

    fn as_brightness(&self) -> Option<&dyn BrightnessControl> {
        Some(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tv_rs_core::conformance;
    use tv_rs_core::{DeviceDescriptor, DeviceId, DeviceOs};

    #[tokio::test]
    async fn ddcci_device_handle_satisfies_core_conformance_suite() {
        let descriptor = DeviceDescriptor::new(
            DeviceId::new("ddcci-test-1").expect("valid device id"),
            DeviceOs::Unknown,
            "local-i2c-1",
        );
        let handle = DdcCiDeviceHandle::mock(descriptor);
        let violations = conformance::run(&handle).await;
        assert!(
            violations.is_empty(),
            "Conformance violations: {violations:#?}"
        );
    }

    #[tokio::test]
    async fn power_input_and_brightness_controls() {
        let descriptor = DeviceDescriptor::new(
            DeviceId::new("ddcci-test-2").expect("valid device id"),
            DeviceOs::Unknown,
            "local-i2c-1",
        );
        let handle = DdcCiDeviceHandle::mock(descriptor);
        let cmd = CommandId::new();

        // Brightness test
        assert_eq!(handle.brightness().await.unwrap(), Some(50));
        handle.set_brightness(85, cmd).await.unwrap();
        assert_eq!(handle.brightness().await.unwrap(), Some(85));

        // Input test
        assert_eq!(handle.active_input().await.unwrap(), Some(Input::Hdmi(1)));
        handle.select_input(Input::Hdmi(2), cmd).await.unwrap();
        assert_eq!(handle.active_input().await.unwrap(), Some(Input::Hdmi(2)));

        // Power test
        assert_eq!(handle.power_state().await.unwrap(), PowerState::On);
        handle.sleep().await.unwrap();
        assert_eq!(handle.power_state().await.unwrap(), PowerState::Off);
        handle.wake().await.unwrap();
        assert_eq!(handle.power_state().await.unwrap(), PowerState::On);
    }
}
