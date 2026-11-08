//! `CecDriver` implementing [`tv_rs_core::driver::Driver`].

use std::sync::Arc;
use async_trait::async_trait;
use tv_rs_core::capability::{Capability, CapabilitySet};
use tv_rs_core::command::{Command, CommandAck, CommandId};
use tv_rs_core::descriptor::{DeviceDescriptor, DeviceId, DeviceOs, DiscoveredTv, PowerState};
use tv_rs_core::driver::Driver;
use tv_rs_core::error::TvError;
use tv_rs_core::observe::{ObservationStream, TvEvent};
use tv_rs_core::types::TvVendor;

use super::handle::CecDeviceHandle;
use super::opcode::*;
use super::transport::{CecTransport, ClientTransport};
use super::types::CecAddress;

/// HDMI-CEC protocol driver.
pub struct CecDriver {
    capabilities: CapabilitySet,
    target_address: CecAddress,
    initiator_address: u8,
    transport: Arc<dyn CecTransport>,
    event_tx: tokio::sync::broadcast::Sender<TvEvent>,
}

impl CecDriver {
    /// Creates a default CEC driver targeting `CecAddress::Tv` via `cec-client`.
    #[must_use]
    pub fn new() -> Self {
        let mut caps = CapabilitySet::empty();
        caps.insert(Capability::PowerState);
        caps.insert(Capability::Wake);
        caps.insert(Capability::Key);
        caps.insert(Capability::Volume);
        caps.insert(Capability::InputSelect);

        let (event_tx, _) = tokio::sync::broadcast::channel(crate::observe::DEFAULT_OBSERVE_CHANNEL_CAPACITY);
        Self {
            capabilities: caps,
            target_address: CecAddress::Tv,
            initiator_address: CecAddress::Playback1.as_u8(),
            transport: Arc::new(ClientTransport::new("cec-client")),
            event_tx,
        }
    }

    /// Sets the underlying transport implementation.
    #[must_use]
    pub fn with_transport(mut self, transport: Arc<dyn CecTransport>) -> Self {
        self.transport = transport;
        self
    }

    /// Targets a specific logical address on the CEC bus.
    #[must_use]
    pub fn for_address(mut self, address: CecAddress) -> Self {
        self.target_address = address;
        self
    }
}

impl Default for CecDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Driver for CecDriver {
    type Handle = CecDeviceHandle;

    fn vendor(&self) -> TvVendor {
        TvVendor::Cec
    }

    fn capabilities(&self) -> &CapabilitySet {
        &self.capabilities
    }

    async fn connect(&mut self, target: &DiscoveredTv) -> Result<Self::Handle, TvError> {
        let address = if !target.ip.is_empty() {
            CecAddress::try_from(target.ip.as_str()).unwrap_or(self.target_address)
        } else {
            self.target_address
        };

        self.target_address = address;
        let device_id = DeviceId::new(format!("cec-{}", address.as_u8()))
            .unwrap_or_else(|_| DeviceId::anonymous());

        let mut desc = DeviceDescriptor::new(device_id, DeviceOs::Cec, format!("cec:{}", address.as_u8()));
        if let Some(ref name) = target.friendly_name {
            desc = desc.with_name(name.clone());
        }

        let _ = self.event_tx.send(TvEvent::ConnectionChanged(true));
        Ok(CecDeviceHandle::new(
            desc,
            self.capabilities,
            self.transport.clone(),
            address,
            self.initiator_address,
        ))
    }

    async fn send(&mut self, cmd: Command) -> Result<CommandAck, TvError> {
        let dest = self.target_address.as_u8();
        let init = self.initiator_address;

        match cmd {
            Command::Power(true) => {
                let frame = image_view_on_frame(init, dest);
                self.transport.send(&frame).await.map_err(|e| crate::map::cec::to_tv_error(&e))?;
                let _ = self.event_tx.send(TvEvent::PowerChanged(PowerState::On));
            }
            Command::Power(false) => {
                let frame = standby_frame(init, dest);
                self.transport.send(&frame).await.map_err(|e| crate::map::cec::to_tv_error(&e))?;
                let _ = self.event_tx.send(TvEvent::PowerChanged(PowerState::Standby));
            }
            Command::Key(key) => {
                let code = map_key_to_cec(key)?;
                let press = user_control_pressed_frame(init, dest, code);
                let release = user_control_released_frame(init, dest);
                self.transport.send(&press).await.map_err(|e| crate::map::cec::to_tv_error(&e))?;
                self.transport.send(&release).await.map_err(|e| crate::map::cec::to_tv_error(&e))?;
            }
            Command::Volume(level) => {
                let code = if level > 50 { CEC_USER_CONTROL_VOLUME_UP } else { CEC_USER_CONTROL_VOLUME_DOWN };
                let press = user_control_pressed_frame(init, dest, code);
                let release = user_control_released_frame(init, dest);
                self.transport.send(&press).await.map_err(|e| crate::map::cec::to_tv_error(&e))?;
                self.transport.send(&release).await.map_err(|e| crate::map::cec::to_tv_error(&e))?;
                let _ = self.event_tx.send(TvEvent::VolumeChanged { level, muted: false });
            }
            Command::Mute(muted) => {
                let press = user_control_pressed_frame(init, dest, CEC_USER_CONTROL_MUTE);
                let release = user_control_released_frame(init, dest);
                self.transport.send(&press).await.map_err(|e| crate::map::cec::to_tv_error(&e))?;
                self.transport.send(&release).await.map_err(|e| crate::map::cec::to_tv_error(&e))?;
                let _ = self.event_tx.send(TvEvent::VolumeChanged { level: 0, muted });
            }
            Command::Input(input) => {
                let phys = map_input_to_physical(input.clone())?;
                let frame = active_source_frame(init, phys);
                self.transport.send(&frame).await.map_err(|e| crate::map::cec::to_tv_error(&e))?;
                let _ = self.event_tx.send(TvEvent::InputChanged(input));
            }
            Command::Custom { name, payload } => {
                if name == "raw" || name == "tx" {
                    self.transport.send(&payload).await.map_err(|e| crate::map::cec::to_tv_error(&e))?;
                } else if name == "active_source" {
                    let phys = u16::from_str_radix(payload.trim_start_matches("0x"), 16)
                        .map_err(|_| TvError::protocol("invalid physical address"))?;
                    let frame = active_source_frame(init, phys);
                    self.transport.send(&frame).await.map_err(|e| crate::map::cec::to_tv_error(&e))?;
                } else {
                    return Err(TvError::Unsupported(Capability::Key));
                }
            }
            Command::Text(_) | Command::LaunchApp(_) => {
                return Err(TvError::Unsupported(Capability::AppLaunch));
            }
        }

        Ok(CommandAck::with_id(CommandId::new()))
    }

    async fn observe(&mut self) -> Result<ObservationStream, TvError> {
        let rx = self.event_tx.subscribe();
        Ok(crate::observe::BroadcastReceiver::new(rx).into_stream())
    }
}
