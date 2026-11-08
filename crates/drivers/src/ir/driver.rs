//! Universal IR Remote Driver implementation.

use std::sync::Arc;
use async_trait::async_trait;
use tv_rs_core::capability::{Capability, CapabilitySet, Key};
use tv_rs_core::command::{Command, CommandAck, CommandId};
use tv_rs_core::descriptor::{DiscoveredTv, PowerState};
use tv_rs_core::driver::Driver;
use tv_rs_core::error::{ProtocolError, TvError};
use tv_rs_core::observe::{ObservationStream, TvEvent};
use tv_rs_core::types::TvVendor;

use crate::ir::device::IrDeviceHandle;
use crate::ir::emitter::{IrEmitter, MockIrEmitter};
use crate::ir::profile::{all_default_profiles, default_samsung_profile, IrProfile};

/// Universal IR remote driver.
pub struct IrDriver {
    capabilities: CapabilitySet,
    emitter: Arc<dyn IrEmitter>,
    active_profile: IrProfile,
    event_tx: tokio::sync::broadcast::Sender<TvEvent>,
}

impl IrDriver {
    /// Creates a new IR driver with a mock emitter and default Samsung profile.
    #[must_use]
    pub fn new() -> Self {
        Self::with_emitter(Arc::new(MockIrEmitter::new()))
    }

    /// Creates a driver with a specific emitter.
    #[must_use]
    pub fn with_emitter(emitter: Arc<dyn IrEmitter>) -> Self {
        let mut caps = CapabilitySet::empty();
        caps.insert(Capability::PowerState);
        caps.insert(Capability::Wake);
        caps.insert(Capability::Key);
        caps.insert(Capability::Volume);
        caps.insert(Capability::InputSelect);

        let (event_tx, _) = tokio::sync::broadcast::channel(crate::observe::DEFAULT_OBSERVE_CHANNEL_CAPACITY);
        Self {
            capabilities: caps,
            emitter,
            active_profile: default_samsung_profile(),
            event_tx,
        }
    }

    /// Sets the active profile for key generation.
    pub fn set_profile(&mut self, profile: IrProfile) {
        self.active_profile = profile;
    }

    /// Returns a reference to the active profile.
    #[must_use]
    pub fn profile(&self) -> &IrProfile {
        &self.active_profile
    }

    /// Returns a reference to the broadcast event sender.
    #[must_use]
    pub fn event_sender(&self) -> &tokio::sync::broadcast::Sender<TvEvent> {
        &self.event_tx
    }
}

impl Default for IrDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Driver for IrDriver {
    type Handle = IrDeviceHandle<Arc<dyn IrEmitter>>;

    fn vendor(&self) -> TvVendor {
        TvVendor::Ir
    }

    fn capabilities(&self) -> &CapabilitySet {
        &self.capabilities
    }

    async fn connect(&mut self, target: &DiscoveredTv) -> Result<Self::Handle, TvError> {
        let brand_match = all_default_profiles().into_iter().find(|p| {
            let label = target.label().to_ascii_lowercase();
            label.contains(&p.brand) || target.ip.to_ascii_lowercase().contains(&p.brand)
        });

        if let Some(profile) = brand_match {
            self.active_profile = profile;
        }

        let _ = self.event_tx.send(TvEvent::ConnectionChanged(true));
        Ok(IrDeviceHandle::new(
            self.active_profile.clone(),
            Arc::new(self.emitter.clone()),
        ))
    }

    async fn send(&mut self, cmd: Command) -> Result<CommandAck, TvError> {
        let key = match cmd {
            Command::Key(k) => k,
            Command::Power(true) => {
                let _ = self.event_tx.send(TvEvent::PowerChanged(PowerState::On));
                Key::Power
            }
            Command::Power(false) => {
                let _ = self.event_tx.send(TvEvent::PowerChanged(PowerState::Standby));
                Key::PowerOff
            }
            Command::Volume(level) => {
                let _ = self.event_tx.send(TvEvent::VolumeChanged { level, muted: false });
                if level > 50 {
                    Key::VolumeUp
                } else {
                    Key::VolumeDown
                }
            }
            Command::Mute(muted) => {
                let _ = self.event_tx.send(TvEvent::VolumeChanged { level: 0, muted });
                Key::Mute
            }
            Command::Custom { name, .. } => {
                return Err(TvError::Protocol(ProtocolError::Malformed(format!(
                    "custom command '{name}' not supported by IR"
                ))));
            }
            _ => return Err(TvError::Unsupported(Capability::Key)),
        };

        let signal = self
            .active_profile
            .signal_for_key(key)
            .map_err(|e| TvError::Protocol(ProtocolError::Malformed(e.to_string())))?;

        self.emitter
            .emit(&signal)
            .await
            .map_err(|e| TvError::Transport(tv_rs_core::error::TransportError::Unreachable(e.to_string())))?;

        Ok(CommandAck::with_id(CommandId::new()))
    }

    async fn observe(&mut self) -> Result<ObservationStream, TvError> {
        let rx = self.event_tx.subscribe();
        Ok(crate::observe::BroadcastReceiver::new(rx).into_stream())
    }
}
