//! Samsung Multiple Display Control (MDC) commercial driver.

use async_trait::async_trait;
use tv_rs_core::capability::{Capability, CapabilitySet};
use tv_rs_core::command::{Command, CommandAck, CommandId};
use tv_rs_core::descriptor::{DeviceDescriptor, DeviceId, DeviceOs, DiscoveredTv, PowerState};
use tv_rs_core::driver::Driver;
use tv_rs_core::error::{ProtocolError, TransportError, TvError};
use tv_rs_core::observe::{ObservationStream, TvEvent};
use tv_rs_core::types::TvVendor;

use crate::mdc::client::{MdcClient, MdcOptions};
use crate::mdc::handle::MdcDevice;
use crate::mdc::types::{DisplayId, InputSource};

/// Samsung commercial display driver operating over MDC (TCP 1515 / RS-232).
pub struct MdcDriver {
    capabilities: CapabilitySet,
    active_client: Option<MdcClient>,
    event_tx: tokio::sync::broadcast::Sender<TvEvent>,
}

impl MdcDriver {
    /// Creates a new unattached MDC driver instance.
    #[must_use]
    pub fn new() -> Self {
        let mut caps = CapabilitySet::empty();
        caps.insert(Capability::PowerState);
        caps.insert(Capability::InputSelect);
        caps.insert(Capability::Volume);
        caps.insert(Capability::Brightness);
        caps.insert(Capability::Reboot);
        caps.insert(Capability::PanelLock);
        caps.insert(Capability::Orientation);
        caps.insert(Capability::NoSignalStatus);

        #[cfg(feature = "supporting")]
        {
            caps.insert(Capability::LampOrPanelHours);
        }

        let (event_tx, _) = tokio::sync::broadcast::channel(crate::observe::DEFAULT_OBSERVE_CHANNEL_CAPACITY);
        Self {
            capabilities: caps,
            active_client: None,
            event_tx,
        }
    }

    /// Attaches an existing connected MDC client for direct driver dispatch.
    #[must_use]
    pub fn with_client(client: MdcClient) -> Self {
        let mut driver = Self::new();
        driver.active_client = Some(client);
        driver
    }

    /// Returns a reference to the broadcast event sender.
    #[must_use]
    pub fn event_sender(&self) -> &tokio::sync::broadcast::Sender<TvEvent> {
        &self.event_tx
    }
}

impl Default for MdcDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Driver for MdcDriver {
    type Handle = MdcDevice;

    fn vendor(&self) -> TvVendor {
        TvVendor::Mdc
    }

    fn capabilities(&self) -> &CapabilitySet {
        &self.capabilities
    }

    async fn connect(&mut self, target: &DiscoveredTv) -> Result<Self::Handle, TvError> {
        let address = target.ip.as_str();
        if address.trim().is_empty() {
            return Err(TvError::protocol("MDC target address must not be empty"));
        }

        let (host, port) = if let Some((h, p)) = address.rsplit_once(':') {
            if let Ok(parsed_port) = p.parse::<u16>() {
                (h.to_string(), parsed_port)
            } else {
                (address.to_string(), crate::mdc::protocol::MDC_TCP_PORT)
            }
        } else {
            (address.to_string(), crate::mdc::protocol::MDC_TCP_PORT)
        };

        let client = MdcClient::with_options(
            host,
            MdcOptions {
                port,
                ..MdcOptions::default()
            },
        );

        let device_id = DeviceId::new(format!("mdc:{address}")).unwrap_or_else(|_| DeviceId::anonymous());
        let descriptor = DeviceDescriptor::new(device_id, DeviceOs::SamsungMdc, address);
        let device = MdcDevice::new(client.clone(), descriptor);

        self.active_client = Some(client);
        let _ = self.event_tx.send(TvEvent::ConnectionChanged(true));
        Ok(device)
    }

    async fn send(&mut self, cmd: Command) -> Result<CommandAck, TvError> {
        let client = self.active_client.as_ref().ok_or_else(|| {
            TvError::Transport(TransportError::Unreachable(
                "MDC driver not connected; call connect() before send()".to_string(),
            ))
        })?;

        match cmd {
            Command::Power(on) => {
                if on {
                    client.power_on().await?;
                    let _ = self.event_tx.send(TvEvent::PowerChanged(PowerState::On));
                } else {
                    client.power_off().await?;
                    let _ = self.event_tx.send(TvEvent::PowerChanged(PowerState::Standby));
                }
                Ok(CommandAck::with_id(CommandId::new()))
            }
            Command::Volume(level) => {
                client.set_volume(level).await?;
                let _ = self.event_tx.send(TvEvent::VolumeChanged { level, muted: false });
                Ok(CommandAck::with_id(CommandId::new()))
            }
            Command::Mute(muted) => {
                client.set_mute(muted).await?;
                let _ = self.event_tx.send(TvEvent::VolumeChanged { level: 0, muted });
                Ok(CommandAck::with_id(CommandId::new()))
            }
            Command::Input(input) => {
                let source = InputSource::from_core_input(&input).ok_or_else(|| {
                    TvError::Protocol(ProtocolError::Unexpected(format!(
                        "MDC has no input code mapping for {input}"
                    )))
                })?;
                client.set_input(source).await?;
                let _ = self.event_tx.send(TvEvent::InputChanged(input));
                Ok(CommandAck::with_id(CommandId::new()))
            }
            Command::Custom { name, payload } => match name.as_str() {
                "video_wall" | "tile" => {
                    let parts: Vec<&str> = payload.split(',').map(str::trim).collect();
                    if parts.len() == 4 {
                        let enable: u8 = parts[0].parse().unwrap_or(1);
                        let div_x: u8 = parts[1].parse().unwrap_or(1);
                        let div_y: u8 = parts[2].parse().unwrap_or(1);
                        let pos: u8 = parts[3].parse().unwrap_or(1);
                        client
                            .request(crate::mdc::protocol::commands::VIDEO_WALL_CONTROL, &[enable, div_x, div_y, pos])
                            .await?;
                        Ok(CommandAck::with_id(CommandId::new()))
                    } else {
                        Err(TvError::protocol("video_wall payload format: enable,div_x,div_y,pos"))
                    }
                }
                "display_id" => {
                    let id: u8 = payload.trim().parse().map_err(|_| TvError::protocol("invalid display id"))?;
                    let display_id = DisplayId::new(id)?;
                    self.active_client = Some(client.display(display_id));
                    Ok(CommandAck::with_id(CommandId::new()))
                }
                "brightness" => {
                    let level: u8 = payload.trim().parse().map_err(|_| TvError::protocol("invalid brightness level"))?;
                    client.set_brightness(level).await?;
                    Ok(CommandAck::with_id(CommandId::new()))
                }
                "panel_lock" => {
                    let locked: bool = payload.trim().parse().unwrap_or(true);
                    client.set_panel_lock(locked).await?;
                    Ok(CommandAck::with_id(CommandId::new()))
                }
                "reboot" => {
                    client.reboot().await?;
                    Ok(CommandAck::with_id(CommandId::new()))
                }
                other => Ok(CommandAck {
                    id: None,
                    message: Some(format!("Unsupported custom command: {other}")),
                }),
            },
            Command::Key(_) => Err(TvError::Unsupported(Capability::Key)),
            Command::Text(_) => Err(TvError::Unsupported(Capability::TextInput)),
            Command::LaunchApp(_) => Err(TvError::Unsupported(Capability::AppLaunch)),
        }
    }

    async fn observe(&mut self) -> Result<ObservationStream, TvError> {
        let rx = self.event_tx.subscribe();
        Ok(crate::observe::BroadcastReceiver::new(rx).into_stream())
    }
}
