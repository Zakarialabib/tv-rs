//! `CastDriver` implementing [`tv_rs_core::driver::Driver`].

use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;
use tv_rs_core::capability::{Capability, CapabilitySet, DeviceHandle, Key};
use tv_rs_core::command::{Command, CommandAck, CommandId};
use tv_rs_core::descriptor::{DeviceDescriptor, DeviceId, DeviceOs, DiscoveredTv, PowerState};
use tv_rs_core::driver::Driver;
use tv_rs_core::error::{TransportError, TvError};
use tv_rs_core::observe::{ObservationStream, TvEvent};
use tv_rs_core::types::TvVendor;

use super::client::CastClient;
use super::dial::DialClient;
use super::handle::CastDeviceHandle;

/// Google Cast TV and Chromecast protocol driver.
pub struct CastDriver {
    capabilities: CapabilitySet,
    active_handle: Option<Arc<CastDeviceHandle>>,
    event_tx: tokio::sync::broadcast::Sender<TvEvent>,
}

impl CastDriver {
    /// Creates a new `CastDriver`.
    #[must_use]
    pub fn new() -> Self {
        let mut caps = CapabilitySet::empty();
        caps.insert(Capability::PowerState);
        caps.insert(Capability::Wake);
        caps.insert(Capability::AppLaunch);
        caps.insert(Capability::Volume);
        caps.insert(Capability::ScreenCast);

        let (event_tx, _) =
            tokio::sync::broadcast::channel(crate::observe::DEFAULT_OBSERVE_CHANNEL_CAPACITY);
        Self {
            capabilities: caps,
            active_handle: None,
            event_tx,
        }
    }

    /// Returns a reference to the broadcast event sender.
    #[must_use]
    pub fn event_sender(&self) -> &tokio::sync::broadcast::Sender<TvEvent> {
        &self.event_tx
    }
}

impl Default for CastDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Driver for CastDriver {
    type Handle = CastDeviceHandle;

    fn vendor(&self) -> TvVendor {
        TvVendor::GoogleCast
    }

    fn capabilities(&self) -> &CapabilitySet {
        &self.capabilities
    }

    async fn connect(&mut self, target: &DiscoveredTv) -> Result<Self::Handle, TvError> {
        let ip = if target.ip.is_empty() {
            return Err(TvError::protocol("Cast target requires valid IP"));
        } else {
            target.ip.clone()
        };

        let client = CastClient::connect(&ip)
            .await
            .map_err(|e| crate::map::cast::to_tv_error(&e, Capability::ScreenCast))?;
        let dial = Arc::new(DialClient::new(&ip));

        let device_id =
            DeviceId::new(format!("cast-{}", ip)).unwrap_or_else(|_| DeviceId::anonymous());
        let mut desc = DeviceDescriptor::new(device_id, DeviceOs::GoogleCast, &ip);
        if let Some(ref name) = target.friendly_name {
            desc = desc.with_name(name.clone());
        }
        if let Some(ref model) = target.model_name {
            desc = desc.with_model(model.clone());
        }

        let handle = CastDeviceHandle::new(
            desc,
            self.capabilities,
            Arc::new(Mutex::new(client)),
            Some(dial),
        );
        let arc_handle = Arc::new(handle);
        self.active_handle = Some(arc_handle.clone());

        let _ = self.event_tx.send(TvEvent::ConnectionChanged(true));
        Ok(CastDeviceHandle::new(
            arc_handle.descriptor(),
            arc_handle.capabilities(),
            arc_handle.client(),
            arc_handle.dial(),
        ))
    }

    async fn send(&mut self, cmd: Command) -> Result<CommandAck, TvError> {
        let handle = self.active_handle.as_ref().ok_or_else(|| {
            TvError::Transport(TransportError::Unreachable(
                "Cast driver not connected to a target device".to_string(),
            ))
        })?;

        match cmd {
            Command::Power(false) => {
                handle.stop_media().await?;
                let _ = self
                    .event_tx
                    .send(TvEvent::PowerChanged(PowerState::Standby));
            }
            Command::LaunchApp(app_id) => {
                use tv_rs_core::capability::AppLaunch;
                handle
                    .launch(
                        tv_rs_core::capability::AppRef::new(app_id.clone()),
                        CommandId::new(),
                    )
                    .await?;
                let _ = self.event_tx.send(TvEvent::AppChanged(app_id));
            }
            Command::Volume(level) => {
                use tv_rs_core::capability::VolumeControl;
                handle.set_volume(level, CommandId::new()).await?;
                let _ = self.event_tx.send(TvEvent::VolumeChanged {
                    level,
                    muted: false,
                });
            }
            Command::Mute(muted) => {
                use tv_rs_core::capability::VolumeControl;
                handle.set_mute(muted, CommandId::new()).await?;
                let _ = self
                    .event_tx
                    .send(TvEvent::VolumeChanged { level: 0, muted });
            }
            Command::Key(Key::Play) => {
                handle.play().await?;
                let _ = self
                    .event_tx
                    .send(TvEvent::MediaPlaybackChanged("PLAYING".to_string()));
            }
            Command::Key(Key::Pause) => {
                handle.pause().await?;
                let _ = self
                    .event_tx
                    .send(TvEvent::MediaPlaybackChanged("PAUSED".to_string()));
            }
            Command::Key(Key::Stop) => {
                handle.stop_media().await?;
                let _ = self
                    .event_tx
                    .send(TvEvent::MediaPlaybackChanged("IDLE".to_string()));
            }
            Command::Custom { name, payload } => {
                if name == "media_status" {
                    let status = handle.media_status().await?;
                    let mut ack = CommandAck::with_id(CommandId::new());
                    ack.message = serde_json::to_string(&status).ok();
                    return Ok(ack);
                } else if name == "play" {
                    handle.play().await?;
                    let _ = self
                        .event_tx
                        .send(TvEvent::MediaPlaybackChanged("PLAYING".to_string()));
                } else if name == "pause" {
                    handle.pause().await?;
                    let _ = self
                        .event_tx
                        .send(TvEvent::MediaPlaybackChanged("PAUSED".to_string()));
                } else if name == "stop" {
                    handle.stop_media().await?;
                    let _ = self
                        .event_tx
                        .send(TvEvent::MediaPlaybackChanged("IDLE".to_string()));
                } else if name == "seek" {
                    let secs = payload
                        .parse::<f64>()
                        .map_err(|_| TvError::protocol("invalid seek seconds"))?;
                    handle.seek(secs).await?;
                } else {
                    return Err(TvError::Unsupported(Capability::ScreenCast));
                }
            }
            _ => return Err(TvError::Unsupported(Capability::ScreenCast)),
        }

        Ok(CommandAck::with_id(CommandId::new()))
    }

    async fn observe(&mut self) -> Result<ObservationStream, TvError> {
        let rx = self.event_tx.subscribe();
        Ok(crate::observe::BroadcastReceiver::new(rx).into_stream())
    }
}
