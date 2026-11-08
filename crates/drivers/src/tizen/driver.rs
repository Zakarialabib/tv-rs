//! `TizenDriver` and `TizenDeviceHandle` implementations.

use std::path::PathBuf;
use std::sync::Arc;
use async_trait::async_trait;
use tv_rs_core::capability::{
    AppLaunch, AppRef, Capability, CapabilitySet, DeviceHandle,
    Key, KeyInput, PowerControl, TextInput, VolumeControl,
};
use tv_rs_core::command::{Command, CommandAck, CommandId};
use tv_rs_core::descriptor::{DeviceDescriptor, DeviceId, DeviceOs, DiscoveredTv, PowerState};
use tv_rs_core::driver::Driver;
use tv_rs_core::error::{TransportError, TvError};
use tv_rs_core::observe::{ObservationStream, TvEvent};
use tv_rs_core::types::TvVendor;

use crate::tizen::remote::{normalize_host, TizenRemote};
use crate::tizen::types::TizenRemoteKey;

/// Tizen smart TV driver.
pub struct TizenDriver {
    capabilities: CapabilitySet,
    remote: Arc<TizenRemote>,
    active_target: Option<String>,
    event_tx: tokio::sync::broadcast::Sender<TvEvent>,
}

impl TizenDriver {
    /// Creates a new Tizen driver with default token store.
    #[must_use]
    pub fn new() -> Self {
        Self::with_token_store(std::env::temp_dir().join("tv-rs-tizen-tokens.json"))
    }

    /// Creates a new Tizen driver with a specific token store path.
    #[must_use]
    pub fn with_token_store(path: impl Into<PathBuf>) -> Self {
        let mut caps = CapabilitySet::empty();
        caps.insert(Capability::PowerState);
        caps.insert(Capability::Wake);
        caps.insert(Capability::Key);
        caps.insert(Capability::AppLaunch);
        caps.insert(Capability::AppList);
        caps.insert(Capability::Foreground);
        caps.insert(Capability::TextInput);
        caps.insert(Capability::Volume);

        let (event_tx, _) = tokio::sync::broadcast::channel(crate::observe::DEFAULT_OBSERVE_CHANNEL_CAPACITY);
        let remote = Arc::new(TizenRemote::new(path, "tv-rs"));
        Self {
            capabilities: caps,
            remote,
            active_target: None,
            event_tx,
        }
    }

    /// Returns a reference to the broadcast event sender.
    #[must_use]
    pub fn event_sender(&self) -> &tokio::sync::broadcast::Sender<TvEvent> {
        &self.event_tx
    }
}

impl Default for TizenDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Driver for TizenDriver {
    type Handle = TizenDeviceHandle;

    fn vendor(&self) -> TvVendor {
        TvVendor::Samsung
    }

    fn capabilities(&self) -> &CapabilitySet {
        &self.capabilities
    }

    async fn connect(&mut self, target: &DiscoveredTv) -> Result<Self::Handle, TvError> {
        let host = if target.ip.is_empty() {
            return Err(TvError::protocol("Tizen target requires valid IP"));
        } else {
            normalize_host(&target.ip).map_err(|e| crate::map::tizen::to_tv_error(&e, Capability::Key))?
        };

        self.active_target = Some(host.clone());
        let device_id = DeviceId::new(format!("tizen-{}", host))
            .unwrap_or_else(|_| DeviceId::anonymous());
        let mut desc = DeviceDescriptor::new(device_id, DeviceOs::Tizen, &host);
        if let Some(ref name) = target.friendly_name {
            desc = desc.with_name(name.clone());
        }
        if let Some(ref model) = target.model_name {
            desc = desc.with_model(model.clone());
        }

        let _ = self.event_tx.send(TvEvent::ConnectionChanged(true));
        Ok(TizenDeviceHandle::new(desc, self.capabilities, self.remote.clone(), host))
    }

    async fn send(&mut self, cmd: Command) -> Result<CommandAck, TvError> {
        let host = self.active_target.as_ref().ok_or_else(|| {
            TvError::Transport(TransportError::Unreachable(
                "Tizen driver not connected to a target device".to_string(),
            ))
        })?;

        match cmd {
            Command::Key(key) => {
                let tizen_key = map_core_key_to_tizen(key)
                    .ok_or(TvError::Unsupported(Capability::Key))?;
                self.remote.send_key_typed(host, tizen_key).await
                    .map_err(|e| crate::map::tizen::to_tv_error(&e, Capability::Key))?;
            }
            Command::Power(false) => {
                self.remote.send_key_typed(host, TizenRemoteKey::PowerOff).await
                    .map_err(|e| crate::map::tizen::to_tv_error(&e, Capability::PowerState))?;
                let _ = self.event_tx.send(TvEvent::PowerChanged(PowerState::Standby));
            }
            Command::Power(true) => {
                self.remote.send_key_typed(host, TizenRemoteKey::Power).await
                    .map_err(|e| crate::map::tizen::to_tv_error(&e, Capability::PowerState))?;
                let _ = self.event_tx.send(TvEvent::PowerChanged(PowerState::On));
            }
            Command::LaunchApp(app_id) => {
                self.remote.launch_app(host, &app_id).await
                    .map_err(|e| crate::map::tizen::to_tv_error(&e, Capability::AppLaunch))?;
                let _ = self.event_tx.send(TvEvent::AppChanged(app_id));
            }
            Command::Volume(level) => {
                let key = if level > 50 { TizenRemoteKey::VolumeUp } else { TizenRemoteKey::VolumeDown };
                self.remote.send_key_typed(host, key).await
                    .map_err(|e| crate::map::tizen::to_tv_error(&e, Capability::Volume))?;
                let _ = self.event_tx.send(TvEvent::VolumeChanged { level, muted: false });
            }
            Command::Mute(muted) => {
                self.remote.send_key_typed(host, TizenRemoteKey::Mute).await
                    .map_err(|e| crate::map::tizen::to_tv_error(&e, Capability::Volume))?;
                let _ = self.event_tx.send(TvEvent::VolumeChanged { level: 0, muted });
            }
            Command::Text(text) => {
                self.remote.send_key(host, &text).await
                    .map_err(|e| crate::map::tizen::to_tv_error(&e, Capability::TextInput))?;
            }
            _ => return Err(TvError::Unsupported(Capability::Key)),
        }

        Ok(CommandAck::with_id(CommandId::new()))
    }

    async fn observe(&mut self) -> Result<ObservationStream, TvError> {
        let rx = self.event_tx.subscribe();
        Ok(crate::observe::BroadcastReceiver::new(rx).into_stream())
    }
}

/// Device handle driving a Samsung Tizen TV.
pub struct TizenDeviceHandle {
    descriptor: DeviceDescriptor,
    capabilities: CapabilitySet,
    remote: Arc<TizenRemote>,
    host: String,
}

impl TizenDeviceHandle {
    /// Creates a new Tizen device handle.
    #[must_use]
    pub fn new(descriptor: DeviceDescriptor, capabilities: CapabilitySet, remote: Arc<TizenRemote>, host: String) -> Self {
        Self { descriptor, capabilities, remote, host }
    }
}

impl DeviceHandle for TizenDeviceHandle {
    fn id(&self) -> DeviceId { self.descriptor.id.clone() }
    fn descriptor(&self) -> DeviceDescriptor { self.descriptor.clone() }
    fn capabilities(&self) -> CapabilitySet { self.capabilities }
    fn as_power(&self) -> Option<&dyn PowerControl> { Some(self) }
    fn as_key(&self) -> Option<&dyn KeyInput> { Some(self) }
    fn as_app(&self) -> Option<&dyn AppLaunch> { Some(self) }
    fn as_volume(&self) -> Option<&dyn VolumeControl> { Some(self) }
    fn as_text(&self) -> Option<&dyn TextInput> { Some(self) }
    fn as_input(&self) -> Option<&dyn tv_rs_core::capability::InputSelect> { None }
}

#[async_trait]
impl KeyInput for TizenDeviceHandle {
    async fn key_press(&self, key: Key, _cmd: CommandId) -> Result<(), TvError> {
        let k = map_core_key_to_tizen(key).ok_or(TvError::Unsupported(Capability::Key))?;
        self.remote.send_key_typed(&self.host, k).await
            .map_err(|e| crate::map::tizen::to_tv_error(&e, Capability::Key))
    }
}

#[async_trait]
impl PowerControl for TizenDeviceHandle {
    async fn power_state(&self) -> Result<PowerState, TvError> {
        if self.remote.is_connected(&self.host).await {
            Ok(PowerState::On)
        } else {
            Ok(PowerState::Standby)
        }
    }
    async fn wake(&self) -> Result<(), TvError> {
        self.remote.send_key_typed(&self.host, TizenRemoteKey::Power).await
            .map_err(|e| crate::map::tizen::to_tv_error(&e, Capability::Wake))
    }
    async fn sleep(&self) -> Result<(), TvError> {
        self.remote.send_key_typed(&self.host, TizenRemoteKey::PowerOff).await
            .map_err(|e| crate::map::tizen::to_tv_error(&e, Capability::PowerState))
    }
}

#[async_trait]
impl AppLaunch for TizenDeviceHandle {
    async fn launch(&self, app: AppRef, _cmd: CommandId) -> Result<(), TvError> {
        self.remote.launch_app(&self.host, &app.id).await
            .map_err(|e| crate::map::tizen::to_tv_error(&e, Capability::AppLaunch))?;
        Ok(())
    }
    async fn foreground(&self) -> Result<Option<AppRef>, TvError> {
        Ok(None)
    }
    async fn installed_apps(&self) -> Result<Vec<AppRef>, TvError> {
        Ok(vec![
            AppRef::new("netflix").with_name("Netflix"),
            AppRef::new("youtube").with_name("YouTube"),
            AppRef::new("prime").with_name("Prime Video"),
        ])
    }
}

#[async_trait]
impl VolumeControl for TizenDeviceHandle {
    async fn volume(&self) -> Result<Option<u8>, TvError> { Ok(None) }
    async fn set_volume(&self, level: u8, _cmd: CommandId) -> Result<(), TvError> {
        let key = if level > 50 { TizenRemoteKey::VolumeUp } else { TizenRemoteKey::VolumeDown };
        self.remote.send_key_typed(&self.host, key).await
            .map_err(|e| crate::map::tizen::to_tv_error(&e, Capability::Volume))
    }
    async fn set_mute(&self, _muted: bool, _cmd: CommandId) -> Result<(), TvError> {
        self.remote.send_key_typed(&self.host, TizenRemoteKey::Mute).await
            .map_err(|e| crate::map::tizen::to_tv_error(&e, Capability::Volume))
    }
    async fn mute(&self) -> Result<Option<bool>, TvError> { Ok(None) }
}

#[async_trait]
impl TextInput for TizenDeviceHandle {
    async fn type_text(&self, text: &str, _cmd: CommandId) -> Result<(), TvError> {
        self.remote.send_key(&self.host, text).await
            .map_err(|e| crate::map::tizen::to_tv_error(&e, Capability::TextInput))
    }
}

/// Maps a [`tv_rs_core::capability::Key`] to a [`TizenRemoteKey`].
#[must_use]
pub fn map_core_key_to_tizen(key: Key) -> Option<TizenRemoteKey> {
    match key {
        Key::Power => Some(TizenRemoteKey::Power),
        Key::PowerOff => Some(TizenRemoteKey::PowerOff),
        Key::Home => Some(TizenRemoteKey::Home),
        Key::Menu => Some(TizenRemoteKey::Menu),
        Key::Back => Some(TizenRemoteKey::Return),
        Key::Enter | Key::Select => Some(TizenRemoteKey::Enter),
        Key::Up => Some(TizenRemoteKey::Up),
        Key::Down => Some(TizenRemoteKey::Down),
        Key::Left => Some(TizenRemoteKey::Left),
        Key::Right => Some(TizenRemoteKey::Right),
        Key::VolumeUp => Some(TizenRemoteKey::VolumeUp),
        Key::VolumeDown => Some(TizenRemoteKey::VolumeDown),
        Key::Mute => Some(TizenRemoteKey::Mute),
        Key::ChannelUp => Some(TizenRemoteKey::ChannelUp),
        Key::ChannelDown => Some(TizenRemoteKey::ChannelDown),
        Key::Play => Some(TizenRemoteKey::Play),
        Key::Pause => Some(TizenRemoteKey::Pause),
        Key::Rewind => Some(TizenRemoteKey::Rewind),
        Key::FastForward => Some(TizenRemoteKey::FastForward),
        _ => None,
    }
}
