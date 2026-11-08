//! `CecDeviceHandle` implementing erased device handle traits.

use std::sync::Arc;
use async_trait::async_trait;
use tv_rs_core::capability::{
    AppLaunch, CapabilitySet, DeviceHandle,
    Input, InputSelect, Key, KeyInput, PowerControl, ScreenCastProvider, TextInput, VolumeControl,
};
use tv_rs_core::command::CommandId;
use tv_rs_core::descriptor::{DeviceDescriptor, DeviceId, PowerState};
use tv_rs_core::error::TvError;

use super::opcode::*;
use super::transport::CecTransport;
use super::types::CecAddress;

/// Erased handle driving an HDMI-CEC attached display or audio sink.
pub struct CecDeviceHandle {
    descriptor: DeviceDescriptor,
    capabilities: CapabilitySet,
    transport: Arc<dyn CecTransport>,
    target_address: CecAddress,
    initiator_address: u8,
}

impl CecDeviceHandle {
    /// Creates a new `CecDeviceHandle`.
    #[must_use]
    pub fn new(
        descriptor: DeviceDescriptor,
        capabilities: CapabilitySet,
        transport: Arc<dyn CecTransport>,
        target_address: CecAddress,
        initiator_address: u8,
    ) -> Self {
        Self {
            descriptor,
            capabilities,
            transport,
            target_address,
            initiator_address,
        }
    }

    /// Accesses the target logical address.
    #[must_use]
    pub fn target_address(&self) -> CecAddress {
        self.target_address
    }

    /// Accesses the underlying transport.
    #[must_use]
    pub fn transport(&self) -> Arc<dyn CecTransport> {
        self.transport.clone()
    }
}

impl DeviceHandle for CecDeviceHandle {
    fn id(&self) -> DeviceId {
        self.descriptor.id.clone()
    }

    fn descriptor(&self) -> DeviceDescriptor {
        self.descriptor.clone()
    }

    fn capabilities(&self) -> CapabilitySet {
        self.capabilities
    }

    fn as_power(&self) -> Option<&dyn PowerControl> {
        Some(self)
    }

    fn as_key(&self) -> Option<&dyn KeyInput> {
        Some(self)
    }

    fn as_volume(&self) -> Option<&dyn VolumeControl> {
        Some(self)
    }

    fn as_input(&self) -> Option<&dyn InputSelect> {
        Some(self)
    }

    fn as_app(&self) -> Option<&dyn AppLaunch> {
        None
    }

    fn as_text(&self) -> Option<&dyn TextInput> {
        None
    }

    fn as_screen_cast(&self) -> Option<&dyn ScreenCastProvider> {
        None
    }
}

#[async_trait]
impl PowerControl for CecDeviceHandle {
    async fn power_state(&self) -> Result<PowerState, TvError> {
        let frame = give_power_status_frame(self.initiator_address, self.target_address.as_u8());
        match self.transport.send(&frame).await {
            Ok(output) => Ok(parse_power_status(&output)),
            Err(_) => Ok(PowerState::Unknown),
        }
    }

    async fn wake(&self) -> Result<(), TvError> {
        let frame = image_view_on_frame(self.initiator_address, self.target_address.as_u8());
        self.transport
            .send(&frame)
            .await
            .map_err(|e| crate::map::cec::to_tv_error(&e))?;
        Ok(())
    }

    async fn sleep(&self) -> Result<(), TvError> {
        let frame = standby_frame(self.initiator_address, self.target_address.as_u8());
        self.transport
            .send(&frame)
            .await
            .map_err(|e| crate::map::cec::to_tv_error(&e))?;
        Ok(())
    }
}

#[async_trait]
impl KeyInput for CecDeviceHandle {
    async fn key_press(&self, key: Key, _cmd: CommandId) -> Result<(), TvError> {
        let code = map_key_to_cec(key)?;
        let init = self.initiator_address;
        let dest = self.target_address.as_u8();

        let press = user_control_pressed_frame(init, dest, code);
        let release = user_control_released_frame(init, dest);

        self.transport
            .send(&press)
            .await
            .map_err(|e| crate::map::cec::to_tv_error(&e))?;
        self.transport
            .send(&release)
            .await
            .map_err(|e| crate::map::cec::to_tv_error(&e))?;
        Ok(())
    }
}

#[async_trait]
impl VolumeControl for CecDeviceHandle {
    async fn volume(&self) -> Result<Option<u8>, TvError> {
        Ok(None)
    }

    async fn set_volume(&self, level: u8, _cmd: CommandId) -> Result<(), TvError> {
        let code = if level > 50 { CEC_USER_CONTROL_VOLUME_UP } else { CEC_USER_CONTROL_VOLUME_DOWN };
        let init = self.initiator_address;
        let dest = self.target_address.as_u8();

        let press = user_control_pressed_frame(init, dest, code);
        let release = user_control_released_frame(init, dest);

        self.transport.send(&press).await.map_err(|e| crate::map::cec::to_tv_error(&e))?;
        self.transport.send(&release).await.map_err(|e| crate::map::cec::to_tv_error(&e))?;
        Ok(())
    }

    async fn set_mute(&self, _muted: bool, _cmd: CommandId) -> Result<(), TvError> {
        let init = self.initiator_address;
        let dest = self.target_address.as_u8();

        let press = user_control_pressed_frame(init, dest, CEC_USER_CONTROL_MUTE);
        let release = user_control_released_frame(init, dest);

        self.transport.send(&press).await.map_err(|e| crate::map::cec::to_tv_error(&e))?;
        self.transport.send(&release).await.map_err(|e| crate::map::cec::to_tv_error(&e))?;
        Ok(())
    }

    async fn mute(&self) -> Result<Option<bool>, TvError> {
        Ok(None)
    }
}

#[async_trait]
impl InputSelect for CecDeviceHandle {
    async fn active_input(&self) -> Result<Option<Input>, TvError> {
        Ok(None)
    }

    async fn select_input(&self, input: Input, _cmd: CommandId) -> Result<(), TvError> {
        let phys = map_input_to_physical(input)?;
        let frame = active_source_frame(self.initiator_address, phys);
        self.transport
            .send(&frame)
            .await
            .map_err(|e| crate::map::cec::to_tv_error(&e))?;
        Ok(())
    }
}
