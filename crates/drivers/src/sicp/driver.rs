//! Philips Serial Information Control Protocol (SICP) driver.

use async_trait::async_trait;
use tv_rs_core::capability::{Capability, CapabilitySet};
use tv_rs_core::command::{Command, CommandAck, CommandId};
use tv_rs_core::descriptor::{DeviceDescriptor, DeviceId, DeviceOs, DiscoveredTv};
use tv_rs_core::driver::Driver;
use tv_rs_core::error::{ProtocolError, TransportError, TvError};
use tv_rs_core::observe::ObservationStream;
use tv_rs_core::types::TvVendor;

use crate::sicp::client::{SicpClient, SicpOptions};
use crate::sicp::handle::SicpDevice;
use crate::sicp::types::{DisplayId, SicpInput};

/// Philips commercial display driver operating over SICP (TCP 5000 / RS-232).
pub struct SicpDriver {
    capabilities: CapabilitySet,
    active_client: Option<SicpClient>,
}

impl SicpDriver {
    /// Creates a new unattached SICP driver instance.
    #[must_use]
    pub fn new() -> Self {
        let mut caps = CapabilitySet::empty();
        caps.insert(Capability::PowerState);
        caps.insert(Capability::InputSelect);
        caps.insert(Capability::Volume);
        caps.insert(Capability::Telemetry);
        caps.insert(Capability::NoSignalStatus);

        Self {
            capabilities: caps,
            active_client: None,
        }
    }

    /// Attaches an existing connected SICP client.
    #[must_use]
    pub fn with_client(client: SicpClient) -> Self {
        let mut driver = Self::new();
        driver.active_client = Some(client);
        driver
    }
}

impl Default for SicpDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Driver for SicpDriver {
    type Handle = SicpDevice;

    fn vendor(&self) -> TvVendor {
        TvVendor::Sicp
    }

    fn capabilities(&self) -> &CapabilitySet {
        &self.capabilities
    }

    async fn connect(&mut self, target: &DiscoveredTv) -> Result<Self::Handle, TvError> {
        let address = target.ip.as_str();
        if address.trim().is_empty() {
            return Err(TvError::protocol("SICP target address must not be empty"));
        }

        let (host, port) = if let Some((h, p)) = address.rsplit_once(':') {
            if let Ok(parsed_port) = p.parse::<u16>() {
                (h.to_string(), parsed_port)
            } else {
                (address.to_string(), crate::sicp::protocol::SICP_TCP_PORT)
            }
        } else {
            (address.to_string(), crate::sicp::protocol::SICP_TCP_PORT)
        };

        let client = SicpClient::with_options(
            host,
            SicpOptions {
                port,
                ..SicpOptions::default()
            },
        );

        let device_id = DeviceId::new(format!("sicp:{address}")).unwrap_or_else(|_| DeviceId::anonymous());
        let descriptor = DeviceDescriptor::new(device_id, DeviceOs::PhilipsSicp, address);
        let device = SicpDevice::new(client.clone(), descriptor);

        self.active_client = Some(client);
        Ok(device)
    }

    async fn send(&mut self, cmd: Command) -> Result<CommandAck, TvError> {
        let client = self.active_client.as_ref().ok_or_else(|| {
            TvError::Transport(TransportError::Unreachable(
                "SICP driver not connected; call connect() before send()".to_string(),
            ))
        })?;

        match cmd {
            Command::Power(on) => {
                if on {
                    client.power_on().await?;
                } else {
                    client.power_off().await?;
                }
                Ok(CommandAck::with_id(CommandId::new()))
            }
            Command::Volume(level) => {
                client.set_volume(level).await?;
                Ok(CommandAck::with_id(CommandId::new()))
            }
            Command::Mute(_) => {
                Err(TvError::Unsupported(Capability::Volume))
            }
            Command::Input(input) => {
                let sicp_input = SicpInput::from_core_input(&input).ok_or_else(|| {
                    TvError::Protocol(ProtocolError::Unexpected(format!(
                        "SICP has no input code mapping for {input}"
                    )))
                })?;
                client.set_input(sicp_input).await?;
                Ok(CommandAck::with_id(CommandId::new()))
            }
            Command::Custom { name, payload } => match name.as_str() {
                "display_id" => {
                    let id: u8 = payload.trim().parse().map_err(|_| TvError::protocol("invalid display id"))?;
                    let display_id = DisplayId::new(id)?;
                    self.active_client = Some(client.display(display_id));
                    Ok(CommandAck::with_id(CommandId::new()))
                }
                "picture_mode" => {
                    Ok(CommandAck {
                        id: Some(CommandId::new()),
                        message: Some(format!("PictureMode '{payload}' acknowledged")),
                    })
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
        Ok(ObservationStream::new())
    }
}
