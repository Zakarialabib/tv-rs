//! Roku Driver implementation and erased handle adapter.

use std::sync::Arc;
use async_trait::async_trait;
use tv_rs_core::capability::{
    AppLaunch, AppRef, Capability, CapabilitySet, DeviceHandle,
    Input, InputSelect, Key, KeyInput, PowerControl, TextInput,
    VolumeControl,
};
use tv_rs_core::command::{Command, CommandAck, CommandId};
use tv_rs_core::descriptor::{DeviceDescriptor, DeviceId, DeviceOs, DiscoveredTv, PowerState};
use tv_rs_core::driver::Driver;
use tv_rs_core::error::{ProtocolError, TransportError, TvError};
use tv_rs_core::observe::{ObservationStream, TvEvent};
use tv_rs_core::types::TvVendor;

use crate::roku::client::RokuClient;
use crate::roku::error::RokuError;
use crate::roku::types::{RokuKey, RokuPowerMode};

/// Roku ECP television driver.
pub struct RokuDriver {
    capabilities: CapabilitySet,
    active_client: Option<RokuClient>,
    event_tx: tokio::sync::broadcast::Sender<TvEvent>,
}

impl RokuDriver {
    /// Creates a new unattached Roku driver.
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
        caps.insert(Capability::InputSelect);

        let (event_tx, _) = tokio::sync::broadcast::channel(crate::observe::DEFAULT_OBSERVE_CHANNEL_CAPACITY);
        Self {
            capabilities: caps,
            active_client: None,
            event_tx,
        }
    }

    /// Creates a driver attached to a specific host.
    #[must_use]
    pub fn for_host(host: impl Into<String>) -> Self {
        let mut driver = Self::new();
        driver.active_client = Some(RokuClient::new(host));
        driver
    }
}

impl Default for RokuDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Driver for RokuDriver {
    type Handle = RokuDeviceHandle;

    fn vendor(&self) -> TvVendor {
        TvVendor::Roku
    }

    fn capabilities(&self) -> &CapabilitySet {
        &self.capabilities
    }

    async fn connect(&mut self, target: &DiscoveredTv) -> Result<Self::Handle, TvError> {
        let host = if target.ip.is_empty() {
            if let Some(ref loc) = target.location {
                loc.clone()
            } else {
                return Err(TvError::protocol("Roku target requires IP or location URL"));
            }
        } else {
            target.ip.clone()
        };

        let client = RokuClient::new(&host);
        self.active_client = Some(client.clone());

        let device_id = DeviceId::new(format!("roku-{}", host))
            .unwrap_or_else(|_| DeviceId::anonymous());
        let mut desc = DeviceDescriptor::new(device_id, DeviceOs::Roku, &host);
        if let Some(ref name) = target.friendly_name {
            desc = desc.with_name(name.clone());
        }
        if let Some(ref model) = target.model_name {
            desc = desc.with_model(model.clone());
        }

        let _ = self.event_tx.send(TvEvent::ConnectionChanged(true));
        Ok(RokuDeviceHandle::new(desc, self.capabilities, client))
    }

    async fn send(&mut self, cmd: Command) -> Result<CommandAck, TvError> {
        let client = self.active_client.as_ref().ok_or_else(|| {
            TvError::Transport(TransportError::Unreachable(
                "Roku driver not connected to a target device".to_string(),
            ))
        })?;

        match cmd {
            Command::Key(key) => {
                let roku_key = map_core_key_to_roku(key)
                    .ok_or(TvError::Unsupported(Capability::Key))?;
                client.send_keypress(roku_key).await.map_err(map_roku_error)?;
            }
            Command::Power(true) => {
                client.power_on().await.map_err(map_roku_error)?;
                let _ = self.event_tx.send(TvEvent::PowerChanged(PowerState::On));
            }
            Command::Power(false) => {
                client.power_off().await.map_err(map_roku_error)?;
                let _ = self.event_tx.send(TvEvent::PowerChanged(PowerState::Standby));
            }
            Command::LaunchApp(app_id) => {
                client.launch_app(&app_id).await.map_err(map_roku_error)?;
                let _ = self.event_tx.send(TvEvent::AppChanged(app_id));
            }
            Command::Text(text) => {
                client.type_text(&text).await.map_err(map_roku_error)?;
            }
            Command::Volume(level) => {
                let key = if level > 50 {
                    RokuKey::VolumeUp
                } else {
                    RokuKey::VolumeDown
                };
                client.send_keypress(key).await.map_err(map_roku_error)?;
                let _ = self.event_tx.send(TvEvent::VolumeChanged { level, muted: false });
            }
            Command::Mute(muted) => {
                client
                    .send_keypress(RokuKey::VolumeMute)
                    .await
                    .map_err(map_roku_error)?;
                let _ = self.event_tx.send(TvEvent::VolumeChanged { level: 0, muted });
            }
            Command::Input(input) => {
                let app_id = format!("tvinput.{}", input);
                client.launch_app(&app_id).await.map_err(map_roku_error)?;
                let _ = self.event_tx.send(TvEvent::InputChanged(input));
            }
            Command::Custom { name, payload } => {
                if name == "keypress" {
                    if let Some(k) = RokuKey::from_str_name(&payload) {
                        client.send_keypress(k).await.map_err(map_roku_error)?;
                    } else {
                        return Err(TvError::Protocol(ProtocolError::Malformed(format!(
                            "unknown roku key: {payload}"
                        ))));
                    }
                } else {
                    return Err(TvError::Unsupported(Capability::Key));
                }
            }
        }

        Ok(CommandAck::with_id(CommandId::new()))
    }

    async fn observe(&mut self) -> Result<ObservationStream, TvError> {
        let rx = self.event_tx.subscribe();
        Ok(crate::observe::BroadcastReceiver::new(rx).into_stream())
    }
}

/// Erased device handle driving a Roku television.
pub struct RokuDeviceHandle {
    descriptor: DeviceDescriptor,
    capabilities: CapabilitySet,
    client: Arc<RokuClient>,
}

impl RokuDeviceHandle {
    /// Creates a new `RokuDeviceHandle`.
    #[must_use]
    pub fn new(descriptor: DeviceDescriptor, capabilities: CapabilitySet, client: RokuClient) -> Self {
        Self {
            descriptor,
            capabilities,
            client: Arc::new(client),
        }
    }

    /// Returns a reference to the underlying Roku client.
    #[must_use]
    pub fn client(&self) -> &RokuClient {
        &self.client
    }
}

impl DeviceHandle for RokuDeviceHandle {
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

    fn as_app(&self) -> Option<&dyn AppLaunch> {
        Some(self)
    }

    fn as_text(&self) -> Option<&dyn TextInput> {
        Some(self)
    }

    fn as_volume(&self) -> Option<&dyn VolumeControl> {
        Some(self)
    }

    fn as_input(&self) -> Option<&dyn InputSelect> {
        Some(self)
    }
}

#[async_trait]
impl PowerControl for RokuDeviceHandle {
    async fn power_state(&self) -> Result<PowerState, TvError> {
        let mode = self.client.power_state().await.map_err(map_roku_error)?;
        Ok(match mode {
            RokuPowerMode::PowerOn | RokuPowerMode::Ready => PowerState::On,
            RokuPowerMode::Headless | RokuPowerMode::DisplayOff => PowerState::Standby,
            RokuPowerMode::Unknown => PowerState::Unknown,
        })
    }

    async fn wake(&self) -> Result<(), TvError> {
        self.client.power_on().await.map_err(map_roku_error)
    }

    async fn sleep(&self) -> Result<(), TvError> {
        self.client.power_off().await.map_err(map_roku_error)
    }
}

#[async_trait]
impl KeyInput for RokuDeviceHandle {
    async fn key_press(&self, key: Key, _cmd: CommandId) -> Result<(), TvError> {
        let k = map_core_key_to_roku(key).ok_or(TvError::Unsupported(Capability::Key))?;
        self.client.send_keypress(k).await.map_err(map_roku_error)
    }
}

#[async_trait]
impl AppLaunch for RokuDeviceHandle {
    async fn launch(&self, app: AppRef, _cmd: CommandId) -> Result<(), TvError> {
        self.client.launch_app(&app.id).await.map_err(map_roku_error)
    }

    async fn foreground(&self) -> Result<Option<AppRef>, TvError> {
        let active = self.client.query_active_app().await.map_err(map_roku_error)?;
        Ok(active.app_id.map(|id| AppRef::new(id).with_name(active.app_name)))
    }

    async fn installed_apps(&self) -> Result<Vec<AppRef>, TvError> {
        let apps = self.client.query_apps().await.map_err(map_roku_error)?;
        Ok(apps.into_iter().map(|a| AppRef::new(a.id).with_name(a.name)).collect())
    }
}

#[async_trait]
impl TextInput for RokuDeviceHandle {
    async fn type_text(&self, text: &str, _cmd: CommandId) -> Result<(), TvError> {
        self.client.type_text(text).await.map_err(map_roku_error)
    }
}

#[async_trait]
impl VolumeControl for RokuDeviceHandle {
    async fn volume(&self) -> Result<Option<u8>, TvError> {
        Ok(None)
    }

    async fn set_volume(&self, level: u8, _cmd: CommandId) -> Result<(), TvError> {
        let key = if level > 50 { RokuKey::VolumeUp } else { RokuKey::VolumeDown };
        self.client.send_keypress(key).await.map_err(map_roku_error)
    }

    async fn set_mute(&self, _muted: bool, _cmd: CommandId) -> Result<(), TvError> {
        self.client.send_keypress(RokuKey::VolumeMute).await.map_err(map_roku_error)
    }

    async fn mute(&self) -> Result<Option<bool>, TvError> {
        Ok(None)
    }
}

#[async_trait]
impl InputSelect for RokuDeviceHandle {
    async fn active_input(&self) -> Result<Option<Input>, TvError> {
        Ok(None)
    }

    async fn select_input(&self, input: Input, _cmd: CommandId) -> Result<(), TvError> {
        let app_id = format!("tvinput.{}", input);
        self.client.launch_app(&app_id).await.map_err(map_roku_error)
    }
}

/// Maps a `tv_rs_core::capability::Key` to a `RokuKey`.
#[must_use]
pub fn map_core_key_to_roku(key: Key) -> Option<RokuKey> {
    match key {
        Key::Home => Some(RokuKey::Home),
        Key::Rewind => Some(RokuKey::Rev),
        Key::FastForward => Some(RokuKey::Fwd),
        Key::Play => Some(RokuKey::Play),
        Key::Pause => Some(RokuKey::Play),
        Key::Select => Some(RokuKey::Select),
        Key::Left => Some(RokuKey::Left),
        Key::Right => Some(RokuKey::Right),
        Key::Down => Some(RokuKey::Down),
        Key::Up => Some(RokuKey::Up),
        Key::Back => Some(RokuKey::Back),
        Key::Previous => Some(RokuKey::InstantReplay),
        Key::Info | Key::Menu => Some(RokuKey::Info),
        Key::Backspace => Some(RokuKey::Backspace),
        Key::Search => Some(RokuKey::Search),
        Key::Enter => Some(RokuKey::Enter),
        Key::PowerOff => Some(RokuKey::PowerOff),
        Key::Power => Some(RokuKey::PowerOn),
        Key::VolumeDown => Some(RokuKey::VolumeDown),
        Key::VolumeUp => Some(RokuKey::VolumeUp),
        Key::Mute => Some(RokuKey::VolumeMute),
        Key::ChannelUp => Some(RokuKey::ChannelUp),
        Key::ChannelDown => Some(RokuKey::ChannelDown),
        _ => None,
    }
}

fn map_roku_error(err: RokuError) -> TvError {
    crate::map::roku::to_tv_error(&err, Capability::Key)
}
