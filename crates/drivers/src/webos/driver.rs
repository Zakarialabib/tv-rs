//! `WebOsDriver` and `WebOsDeviceHandle` implementations.

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

use crate::webos::client::WebosClient;
use crate::webos::pool::{DynSessionPool, LiveSessionFactory};

/// Driver controlling LG webOS displays over SSAP.
pub struct WebOsDriver {
    capabilities: CapabilitySet,
    pool: Arc<DynSessionPool>,
    active_client: Option<WebosClient>,
    event_tx: tokio::sync::broadcast::Sender<TvEvent>,
}

impl WebOsDriver {
    /// Creates a new webOS driver.
    #[must_use]
    pub fn new() -> Self {
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
        let pool = Arc::new(DynSessionPool::with_factory(Box::new(LiveSessionFactory)));
        Self {
            capabilities: caps,
            pool,
            active_client: None,
            event_tx,
        }
    }

    /// Returns a reference to the broadcast event sender.
    #[must_use]
    pub fn event_sender(&self) -> &tokio::sync::broadcast::Sender<TvEvent> {
        &self.event_tx
    }
}

impl Default for WebOsDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Driver for WebOsDriver {
    type Handle = WebOsDeviceHandle;

    fn vendor(&self) -> TvVendor {
        TvVendor::Lg
    }

    fn capabilities(&self) -> &CapabilitySet {
        &self.capabilities
    }

    async fn connect(&mut self, target: &DiscoveredTv) -> Result<Self::Handle, TvError> {
        let host = if target.ip.is_empty() {
            return Err(TvError::protocol("webOS target requires valid IP"));
        } else {
            target.ip.clone()
        };

        let client = WebosClient::new(&host).with_wss();
        self.active_client = Some(client.clone());

        let device_id = DeviceId::new(format!("webos-{}", host))
            .unwrap_or_else(|_| DeviceId::anonymous());
        let mut desc = DeviceDescriptor::new(device_id, DeviceOs::WebOs, &host);
        if let Some(ref name) = target.friendly_name {
            desc = desc.with_name(name.clone());
        }
        if let Some(ref model) = target.model_name {
            desc = desc.with_model(model.clone());
        }

        let _ = self.event_tx.send(TvEvent::ConnectionChanged(true));
        Ok(WebOsDeviceHandle::new(desc, self.capabilities, self.pool.clone(), client))
    }

    async fn send(&mut self, cmd: Command) -> Result<CommandAck, TvError> {
        let client = self.active_client.as_ref().ok_or_else(|| {
            TvError::Transport(TransportError::Unreachable(
                "webOS driver not connected to a target device".to_string(),
            ))
        })?;

        match cmd {
            Command::Key(key) => {
                let btn = map_core_key_to_webos(key)
                    .ok_or(TvError::Unsupported(Capability::Key))?;
                self.pool.send_key(client, btn).await
                    .map_err(|e| crate::map::webos::to_tv_error(&e, Capability::Key))?;
            }
            Command::Power(false) => {
                self.pool.power_off(client).await
                    .map_err(|e| crate::map::webos::to_tv_error(&e, Capability::PowerState))?;
                let _ = self.event_tx.send(TvEvent::PowerChanged(PowerState::Standby));
            }
            Command::LaunchApp(app_id) => {
                self.pool.launch_app(client, &app_id).await
                    .map_err(|e| crate::map::webos::to_tv_error(&e, Capability::AppLaunch))?;
                let _ = self.event_tx.send(TvEvent::AppChanged(app_id));
            }
            Command::Volume(level) => {
                self.pool.set_volume(client, level).await
                    .map_err(|e| crate::map::webos::to_tv_error(&e, Capability::Volume))?;
                let _ = self.event_tx.send(TvEvent::VolumeChanged { level, muted: false });
            }
            Command::Mute(muted) => {
                self.pool.set_mute(client, muted).await
                    .map_err(|e| crate::map::webos::to_tv_error(&e, Capability::Volume))?;
                let _ = self.event_tx.send(TvEvent::VolumeChanged { level: 0, muted });
            }
            Command::Text(text) => {
                self.pool.insert_text(client, &text, false).await
                    .map_err(|e| crate::map::webos::to_tv_error(&e, Capability::TextInput))?;
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

/// Device handle driving an LG webOS display.
pub struct WebOsDeviceHandle {
    descriptor: DeviceDescriptor,
    capabilities: CapabilitySet,
    pool: Arc<DynSessionPool>,
    client: WebosClient,
}

impl WebOsDeviceHandle {
    /// Creates a new webOS device handle.
    #[must_use]
    pub fn new(descriptor: DeviceDescriptor, capabilities: CapabilitySet, pool: Arc<DynSessionPool>, client: WebosClient) -> Self {
        Self { descriptor, capabilities, pool, client }
    }
}

impl DeviceHandle for WebOsDeviceHandle {
    fn id(&self) -> DeviceId { self.descriptor.id.clone() }
    fn descriptor(&self) -> DeviceDescriptor { self.descriptor.clone() }
    fn capabilities(&self) -> CapabilitySet { self.capabilities }
    fn as_power(&self) -> Option<&dyn PowerControl> { Some(self) }
    fn as_volume(&self) -> Option<&dyn VolumeControl> { Some(self) }
    fn as_key(&self) -> Option<&dyn KeyInput> { Some(self) }
    fn as_app(&self) -> Option<&dyn AppLaunch> { Some(self) }
    fn as_text(&self) -> Option<&dyn TextInput> { Some(self) }
    fn as_input(&self) -> Option<&dyn tv_rs_core::capability::InputSelect> { None }
}

#[async_trait]
impl KeyInput for WebOsDeviceHandle {
    async fn key_press(&self, key: Key, _cmd: CommandId) -> Result<(), TvError> {
        let btn = map_core_key_to_webos(key).ok_or(TvError::Unsupported(Capability::Key))?;
        self.pool.send_key(&self.client, btn).await
            .map_err(|e| crate::map::webos::to_tv_error(&e, Capability::Key))
    }
}

#[async_trait]
impl PowerControl for WebOsDeviceHandle {
    async fn power_state(&self) -> Result<PowerState, TvError> {
        Ok(PowerState::On)
    }
    async fn wake(&self) -> Result<(), TvError> {
        Err(TvError::Unsupported(Capability::Wake))
    }
    async fn sleep(&self) -> Result<(), TvError> {
        self.pool.power_off(&self.client).await
            .map_err(|e| crate::map::webos::to_tv_error(&e, Capability::PowerState))
    }
}

#[async_trait]
impl VolumeControl for WebOsDeviceHandle {
    async fn volume(&self) -> Result<Option<u8>, TvError> {
        self.pool.get_volume(&self.client).await
            .map(Some)
            .map_err(|e| crate::map::webos::to_tv_error(&e, Capability::Volume))
    }
    async fn set_volume(&self, level: u8, _cmd: CommandId) -> Result<(), TvError> {
        self.pool.set_volume(&self.client, level).await
            .map_err(|e| crate::map::webos::to_tv_error(&e, Capability::Volume))
    }
    async fn mute(&self) -> Result<Option<bool>, TvError> {
        self.pool.get_mute(&self.client).await
            .map(Some)
            .map_err(|e| crate::map::webos::to_tv_error(&e, Capability::Volume))
    }
    async fn set_mute(&self, muted: bool, _cmd: CommandId) -> Result<(), TvError> {
        self.pool.set_mute(&self.client, muted).await
            .map_err(|e| crate::map::webos::to_tv_error(&e, Capability::Volume))
    }
}

#[async_trait]
impl AppLaunch for WebOsDeviceHandle {
    async fn launch(&self, app: AppRef, _cmd: CommandId) -> Result<(), TvError> {
        self.pool.launch_app(&self.client, &app.id).await
            .map_err(|e| crate::map::webos::to_tv_error(&e, Capability::AppLaunch))
    }
    async fn foreground(&self) -> Result<Option<AppRef>, TvError> {
        let app_id = self.pool.foreground_app(&self.client).await
            .map_err(|e| crate::map::webos::to_tv_error(&e, Capability::Foreground))?;
        Ok(app_id.map(AppRef::new))
    }
    async fn installed_apps(&self) -> Result<Vec<AppRef>, TvError> {
        let apps = self.pool.list_apps(&self.client).await
            .map_err(|e| crate::map::webos::to_tv_error(&e, Capability::AppList))?;
        Ok(apps.into_iter().map(|a| AppRef::new(a.id).with_name(a.title.unwrap_or_default())).collect())
    }
}

#[async_trait]
impl TextInput for WebOsDeviceHandle {
    async fn type_text(&self, text: &str, _cmd: CommandId) -> Result<(), TvError> {
        self.pool.insert_text(&self.client, text, false).await
            .map_err(|e| crate::map::webos::to_tv_error(&e, Capability::TextInput))
    }
}

/// Maps a [`tv_rs_core::capability::Key`] to a webOS pointer button string.
#[must_use]
pub fn map_core_key_to_webos(key: Key) -> Option<&'static str> {
    match key {
        Key::Home => Some("HOME"),
        Key::Back => Some("BACK"),
        Key::Enter | Key::Select => Some("ENTER"),
        Key::Up => Some("UP"),
        Key::Down => Some("DOWN"),
        Key::Left => Some("LEFT"),
        Key::Right => Some("RIGHT"),
        Key::VolumeUp => Some("VOLUMEUP"),
        Key::VolumeDown => Some("VOLUMEDOWN"),
        Key::Mute => Some("MUTE"),
        Key::Menu => Some("MENU"),
        _ => None,
    }
}
