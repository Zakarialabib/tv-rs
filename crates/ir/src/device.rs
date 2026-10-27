//! `IrDeviceHandle` implementing [`tv_rs_core::DeviceHandle`].

use async_trait::async_trait;
use std::sync::Arc;

use tv_rs_core::{
    AppLaunch, Capability, CapabilitySet, CommandId, DeviceDescriptor, DeviceHandle, DeviceId,
    DeviceOs, Input, InputSelect, Key, KeyInput, PowerControl, PowerState, TextInput, TvError,
    VolumeControl,
};

use crate::emitter::IrEmitter;
use crate::profile::IrProfile;

/// Erased device handle driving a device via IR remote commands.
pub struct IrDeviceHandle<E: IrEmitter> {
    descriptor: DeviceDescriptor,
    profile: IrProfile,
    emitter: Arc<E>,
}

impl<E: IrEmitter + 'static> IrDeviceHandle<E> {
    /// Creates a new `IrDeviceHandle` wrapping an IR profile and emitter.
    #[must_use]
    pub fn new(profile: IrProfile, emitter: Arc<E>) -> Self {
        let device_id = DeviceId::new(format!("ir-{}", profile.brand))
            .unwrap_or_else(|_| DeviceId::anonymous());
        let descriptor = DeviceDescriptor::new(
            device_id,
            DeviceOs::Unknown,
            format!("ir:{}", profile.brand),
        )
        .with_name(profile.name.clone());

        Self {
            descriptor,
            profile,
            emitter,
        }
    }

    /// Accesses the underlying [`IrProfile`].
    #[must_use]
    pub fn profile(&self) -> &IrProfile {
        &self.profile
    }

    /// Accesses the underlying [`IrEmitter`].
    #[must_use]
    pub fn emitter(&self) -> &E {
        &self.emitter
    }
}

impl<E: IrEmitter + 'static> DeviceHandle for IrDeviceHandle<E> {
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
            .with(Capability::Key)
            .with(Capability::Volume)
            .with(Capability::InputSelect)
    }

    fn as_power(&self) -> Option<&dyn PowerControl> {
        Some(self)
    }

    fn as_key(&self) -> Option<&dyn KeyInput> {
        Some(self)
    }

    fn as_app(&self) -> Option<&dyn AppLaunch> {
        None
    }

    fn as_text(&self) -> Option<&dyn TextInput> {
        None
    }

    fn as_volume(&self) -> Option<&dyn VolumeControl> {
        Some(self)
    }

    fn as_input(&self) -> Option<&dyn InputSelect> {
        Some(self)
    }
}

#[async_trait]
impl<E: IrEmitter + 'static> PowerControl for IrDeviceHandle<E> {
    async fn power_state(&self) -> Result<PowerState, TvError> {
        // IR is unidirectional; actual power state is unobservable without feedback probes
        Ok(PowerState::Unknown)
    }

    async fn wake(&self) -> Result<(), TvError> {
        let sig = self
            .profile
            .signal_for_key(Key::Power)
            .map_err(TvError::from)?;
        self.emitter.emit(&sig).await.map_err(TvError::from)
    }

    async fn sleep(&self) -> Result<(), TvError> {
        let sig = self
            .profile
            .signal_for_key(Key::PowerOff)
            .or_else(|_| self.profile.signal_for_key(Key::Power))
            .map_err(TvError::from)?;
        self.emitter.emit(&sig).await.map_err(TvError::from)
    }
}

#[async_trait]
impl<E: IrEmitter + 'static> KeyInput for IrDeviceHandle<E> {
    async fn key_press(&self, key: Key, _cmd: CommandId) -> Result<(), TvError> {
        let sig = self.profile.signal_for_key(key).map_err(TvError::from)?;
        self.emitter.emit(&sig).await.map_err(TvError::from)
    }
}

#[async_trait]
impl<E: IrEmitter + 'static> VolumeControl for IrDeviceHandle<E> {
    async fn volume(&self) -> Result<Option<u8>, TvError> {
        Ok(None)
    }

    async fn set_volume(&self, level: u8, _cmd: CommandId) -> Result<(), TvError> {
        // Send single volume up/down key as step
        let key = if level > 50 {
            Key::VolumeUp
        } else {
            Key::VolumeDown
        };
        let sig = self.profile.signal_for_key(key).map_err(TvError::from)?;
        self.emitter.emit(&sig).await.map_err(TvError::from)
    }

    async fn set_mute(&self, _muted: bool, _cmd: CommandId) -> Result<(), TvError> {
        let sig = self
            .profile
            .signal_for_key(Key::Mute)
            .map_err(TvError::from)?;
        self.emitter.emit(&sig).await.map_err(TvError::from)
    }

    async fn mute(&self) -> Result<Option<bool>, TvError> {
        Ok(None)
    }
}

#[async_trait]
impl<E: IrEmitter + 'static> InputSelect for IrDeviceHandle<E> {
    async fn active_input(&self) -> Result<Option<Input>, TvError> {
        Ok(None)
    }

    async fn select_input(&self, _input: Input, _cmd: CommandId) -> Result<(), TvError> {
        Err(TvError::Unsupported(Capability::InputSelect))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emitter::MockIrEmitter;
    use crate::profile::default_samsung_profile;

    #[tokio::test]
    async fn ir_device_handle_sends_keys_via_emitter() {
        let profile = default_samsung_profile();
        let emitter = Arc::new(MockIrEmitter::new());
        let handle = IrDeviceHandle::new(profile, emitter.clone());

        assert!(handle.capabilities().contains(Capability::Key));
        assert!(handle.as_key().is_some());

        let key_input = handle.as_key().unwrap();
        key_input
            .key_press(Key::Power, CommandId::new())
            .await
            .unwrap();

        assert_eq!(emitter.emitted_signals().len(), 1);
        assert_eq!(emitter.last_emitted().unwrap().command, 0x02);
    }
}
