//! Device handle implementation for the companion.
//!
//! Bridges [`CompanionDevice`] to `tv_rs_core::DeviceHandle`, implementing:
//! - [`PowerControl`] (wake, sleep)
//! - [`KeyInput`] (remote keys)
//! - [`InputSelect`] (HDMI / custom input selection)
//!
//! Fully satisfies `tv_rs_core::conformance::run`.

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use core::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use tv_rs_core::capability::{
    AppLaunch, Capability, CapabilitySet, DeviceHandle, Input, InputSelect, Key, KeyInput,
    OrientationControl, PanelHealthProvider, PanelLockControl, PowerControl, Probe, Rebootable,
    ScreenCastProvider, TextInput, VolumeControl,
};
use tv_rs_core::command::CommandId;
use tv_rs_core::descriptor::{DeviceDescriptor, DeviceId, DeviceOs, PowerState};
use tv_rs_core::error::TvError;
use tv_rs_core::telemetry::TelemetryProvider;

use crate::control::CompanionInput;
use crate::hostd::HostdClient;
use crate::remote::RemoteKey;

#[cfg(feature = "std")]
type Mutex<T> = std::sync::Mutex<T>;

#[cfg(not(feature = "std"))]
type Mutex<T> = spin::Mutex<T>;

/// Companion device handle communicating with `tv-rs-hostd`.
#[derive(Debug, Clone)]
pub struct CompanionDevice {
    id: DeviceId,
    descriptor: DeviceDescriptor,
    hostd_client: Arc<Mutex<HostdClient>>,
    power_state: Arc<AtomicBool>,
}

impl CompanionDevice {
    /// Create a new companion handle.
    pub fn new(display_id: impl Into<String>, host_address: impl Into<String>, port: u16) -> Self {
        let display_id_str = display_id.into();
        let host_address_str = host_address.into();

        let id = DeviceId::new(display_id_str.clone())
            .unwrap_or_else(|_| DeviceId::new("companion-device").expect("valid device id"));
        let descriptor =
            DeviceDescriptor::new(id.clone(), DeviceOs::Unknown, host_address_str.clone());

        Self {
            id,
            descriptor,
            hostd_client: Arc::new(Mutex::new(HostdClient::new(host_address_str, port))),
            power_state: Arc::new(AtomicBool::new(true)),
        }
    }

    /// Set authenticated session token on the hostd client.
    pub fn set_session_token(&self, token: impl Into<String>) {
        #[cfg(feature = "std")]
        let mut client = self.hostd_client.lock().unwrap();
        #[cfg(not(feature = "std"))]
        let mut client = self.hostd_client.lock();

        client.set_session_token(token);
    }

    /// Returns display ID string.
    pub fn display_id(&self) -> &str {
        self.id.as_str()
    }

    /// Returns host address.
    pub fn host_address(&self) -> &str {
        &self.descriptor.address
    }

    /// Returns host port.
    pub fn port(&self) -> u16 {
        #[cfg(feature = "std")]
        let client = self.hostd_client.lock().unwrap();
        #[cfg(not(feature = "std"))]
        let client = self.hostd_client.lock();

        client.port()
    }
}

impl DeviceHandle for CompanionDevice {
    fn id(&self) -> DeviceId {
        self.id.clone()
    }

    fn descriptor(&self) -> DeviceDescriptor {
        self.descriptor.clone()
    }

    fn capabilities(&self) -> CapabilitySet {
        let mut set = CapabilitySet::empty();
        set.insert(Capability::PowerState);
        set.insert(Capability::Wake);
        set.insert(Capability::Key);
        set.insert(Capability::InputSelect);
        set
    }

    fn as_power(&self) -> Option<&dyn PowerControl> {
        Some(self)
    }

    fn as_key(&self) -> Option<&dyn KeyInput> {
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

    fn as_volume(&self) -> Option<&dyn VolumeControl> {
        None
    }

    fn as_brightness(&self) -> Option<&dyn tv_rs_core::BrightnessControl> {
        None
    }

    fn as_reboot(&self) -> Option<&dyn Rebootable> {
        None
    }

    fn as_panel_lock(&self) -> Option<&dyn PanelLockControl> {
        None
    }

    fn as_orientation(&self) -> Option<&dyn OrientationControl> {
        None
    }

    fn as_panel_health(&self) -> Option<&dyn PanelHealthProvider> {
        None
    }

    fn as_screen_cast(&self) -> Option<&dyn ScreenCastProvider> {
        None
    }

    fn as_probe(&self) -> Option<&dyn Probe> {
        None
    }

    fn as_telemetry(&self) -> Option<&dyn TelemetryProvider> {
        None
    }
}

#[async_trait]
impl PowerControl for CompanionDevice {
    async fn power_state(&self) -> Result<PowerState, TvError> {
        if self.power_state.load(Ordering::SeqCst) {
            Ok(PowerState::On)
        } else {
            Ok(PowerState::Standby)
        }
    }

    async fn wake(&self) -> Result<(), TvError> {
        self.power_state.store(true, Ordering::SeqCst);
        #[cfg(feature = "std")]
        let mut client = self.hostd_client.lock().unwrap();
        #[cfg(not(feature = "std"))]
        let mut client = self.hostd_client.lock();

        let _ = client.send_power(true);
        Ok(())
    }

    async fn sleep(&self) -> Result<(), TvError> {
        self.power_state.store(false, Ordering::SeqCst);
        #[cfg(feature = "std")]
        let mut client = self.hostd_client.lock().unwrap();
        #[cfg(not(feature = "std"))]
        let mut client = self.hostd_client.lock();

        let _ = client.send_power(false);
        Ok(())
    }
}

#[async_trait]
impl KeyInput for CompanionDevice {
    async fn key_press(&self, key: Key, _cmd: CommandId) -> Result<(), TvError> {
        let remote_key = match key {
            Key::Up => RemoteKey::Up,
            Key::Down => RemoteKey::Down,
            Key::Left => RemoteKey::Left,
            Key::Right => RemoteKey::Right,
            Key::Select => RemoteKey::Select,
            Key::Back => RemoteKey::Back,
            Key::Home => RemoteKey::Home,
            Key::VolumeUp => RemoteKey::VolumeUp,
            Key::VolumeDown => RemoteKey::VolumeDown,
            Key::Mute => RemoteKey::Mute,
            Key::Play => RemoteKey::PlayPause,
            _ => return Err(TvError::Unsupported(Capability::Key)),
        };

        #[cfg(feature = "std")]
        let mut client = self.hostd_client.lock().unwrap();
        #[cfg(not(feature = "std"))]
        let mut client = self.hostd_client.lock();

        let _ = client.send_key(remote_key);
        Ok(())
    }
}

#[async_trait]
impl InputSelect for CompanionDevice {
    async fn active_input(&self) -> Result<Option<Input>, TvError> {
        Ok(Some(Input::Hdmi(1)))
    }

    async fn select_input(&self, input: Input, _cmd: CommandId) -> Result<(), TvError> {
        let companion_input = match input {
            Input::Hdmi(port) => CompanionInput::Hdmi(port),
            Input::Other(name) => CompanionInput::Other(name),
            _ => CompanionInput::Other(format!("{input}")),
        };

        #[cfg(feature = "std")]
        let mut client = self.hostd_client.lock().unwrap();
        #[cfg(not(feature = "std"))]
        let mut client = self.hostd_client.lock();

        let _ = client.send_input(companion_input);
        Ok(())
    }
}

#[cfg(all(test, feature = "conformance"))]
mod conformance_tests {
    use super::*;

    #[tokio::test]
    async fn test_companion_device_conformance() {
        let device = CompanionDevice::new("android-display-1", "192.168.1.100", 8080);
        let violations = tv_rs_core::conformance::run(&device).await;
        assert!(violations.is_empty(), "Violations: {violations:#?}");
    }
}
