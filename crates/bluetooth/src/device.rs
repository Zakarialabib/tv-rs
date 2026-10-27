//! Bluetooth remote control client implementation.

use std::time::Duration;

use async_trait::async_trait;

use tv_rs_core::{
    AppLaunch, Capability, CapabilitySet, CommandId, DeviceDescriptor, DeviceHandle, DeviceId,
    DeviceOs, InputSelect, Key, KeyInput, PowerControl, PowerState, TextInput, TvError,
    VolumeControl,
};

use crate::error::BluetoothError;
use crate::hid::{map_ascii_char_to_hid, map_key_to_hid, HidKeyboardReport};
use crate::transport::{BluetoothAddress, BluetoothTransport};

/// A Bluetooth HID remote control client that emulates a keyboard to control smart TVs and media boxes.
pub struct BluetoothRemote<T: BluetoothTransport> {
    address: BluetoothAddress,
    label: String,
    transport: T,
    key_press_duration: Duration,
}

impl<T: BluetoothTransport> BluetoothRemote<T> {
    /// Creates a new `BluetoothRemote` with the target device address and transport.
    pub fn new(address: BluetoothAddress, transport: T) -> Self {
        let label = format!("Bluetooth Remote ({address})");
        Self {
            address,
            label,
            transport,
            key_press_duration: Duration::from_millis(50),
        }
    }

    /// Sets a custom label for the device.
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    /// Sets the key press duration between press and release reports.
    pub fn with_key_press_duration(mut self, duration: Duration) -> Self {
        self.key_press_duration = duration;
        self
    }

    /// Access the underlying transport.
    pub fn transport(&self) -> &T {
        &self.transport
    }

    /// The target Bluetooth address.
    pub fn address(&self) -> BluetoothAddress {
        self.address
    }

    async fn ensure_connected(&self) -> Result<(), TvError> {
        if !self.transport.is_connected().await.map_err(TvError::from)? {
            self.transport.connect().await.map_err(TvError::from)?;
        }
        Ok(())
    }
}

#[async_trait]
impl<T: BluetoothTransport> PowerControl for BluetoothRemote<T> {
    async fn power_state(&self) -> Result<PowerState, TvError> {
        match self.transport.is_connected().await {
            Ok(true) => Ok(PowerState::On),
            Ok(false) => Ok(PowerState::Standby),
            Err(_) => Ok(PowerState::Unknown),
        }
    }

    async fn wake(&self) -> Result<(), TvError> {
        self.ensure_connected().await?;
        // Send Enter / Space key press to wake the device
        let (keycode, mods) = (0x28, 0); // Enter
        self.transport
            .send_report(&HidKeyboardReport::key_press(keycode, mods))
            .await
            .map_err(TvError::from)?;
        tokio::time::sleep(self.key_press_duration).await;
        self.transport
            .send_report(&HidKeyboardReport::release_all())
            .await
            .map_err(TvError::from)?;
        Ok(())
    }

    async fn sleep(&self) -> Result<(), TvError> {
        self.ensure_connected().await?;
        self.key_press(Key::Power, CommandId::new()).await?;
        self.transport.disconnect().await.map_err(TvError::from)?;
        Ok(())
    }
}

#[async_trait]
impl<T: BluetoothTransport> KeyInput for BluetoothRemote<T> {
    async fn key_press(&self, key: Key, _cmd: CommandId) -> Result<(), TvError> {
        let (keycode, modifiers) =
            map_key_to_hid(key).ok_or(TvError::Unsupported(Capability::Key))?;
        self.ensure_connected().await?;

        // Send key press report
        self.transport
            .send_report(&HidKeyboardReport::key_press(keycode, modifiers))
            .await
            .map_err(TvError::from)?;

        // Hold key for key_press_duration
        tokio::time::sleep(self.key_press_duration).await;

        // Send key release report
        self.transport
            .send_report(&HidKeyboardReport::release_all())
            .await
            .map_err(TvError::from)?;

        Ok(())
    }
}

#[async_trait]
impl<T: BluetoothTransport> TextInput for BluetoothRemote<T> {
    async fn type_text(&self, text: &str, _cmd: CommandId) -> Result<(), TvError> {
        if text.is_empty() {
            return Ok(());
        }

        self.ensure_connected().await?;

        for ch in text.chars() {
            let (keycode, modifiers) = map_ascii_char_to_hid(ch).ok_or_else(|| {
                TvError::from(BluetoothError::UnsupportedKey(format!(
                    "character '{ch}' cannot be typed via HID"
                )))
            })?;

            self.transport
                .send_report(&HidKeyboardReport::key_press(keycode, modifiers))
                .await
                .map_err(TvError::from)?;

            tokio::time::sleep(Duration::from_millis(20)).await;

            self.transport
                .send_report(&HidKeyboardReport::release_all())
                .await
                .map_err(TvError::from)?;

            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        Ok(())
    }
}

impl<T: BluetoothTransport> DeviceHandle for BluetoothRemote<T> {
    fn id(&self) -> DeviceId {
        DeviceId::new(format!("bluetooth-{}", self.address.as_str()))
            .unwrap_or_else(|_| DeviceId::anonymous())
    }

    fn descriptor(&self) -> DeviceDescriptor {
        DeviceDescriptor::new(self.id(), DeviceOs::Unknown, self.address.as_str())
            .with_name(self.label.clone())
            .with_model("Bluetooth HID Emulation")
    }

    fn capabilities(&self) -> CapabilitySet {
        CapabilitySet::empty()
            .with(Capability::PowerState)
            .with(Capability::Wake)
            .with(Capability::Key)
            .with(Capability::TextInput)
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
        Some(self)
    }

    fn as_input(&self) -> Option<&dyn InputSelect> {
        None
    }

    fn as_volume(&self) -> Option<&dyn VolumeControl> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::MockBluetoothTransport;

    #[tokio::test]
    async fn test_bluetooth_remote_key_press() {
        let mock = MockBluetoothTransport::new();
        let addr = BluetoothAddress::try_from("AA:BB:CC:DD:EE:FF").unwrap();
        let remote =
            BluetoothRemote::new(addr, mock).with_key_press_duration(Duration::from_millis(1));

        remote.key_press(Key::Up, CommandId::new()).await.unwrap();

        let reports = remote.transport().recorded_reports();
        assert_eq!(reports.len(), 2);
        // Press Up Arrow (0x52)
        assert_eq!(reports[0], HidKeyboardReport::key_press(0x52, 0));
        // Release all
        assert_eq!(reports[1], HidKeyboardReport::release_all());
    }

    #[tokio::test]
    async fn test_bluetooth_remote_type_text() {
        let mock = MockBluetoothTransport::new();
        let addr = BluetoothAddress::try_from("AA:BB:CC:DD:EE:FF").unwrap();
        let remote =
            BluetoothRemote::new(addr, mock).with_key_press_duration(Duration::from_millis(1));

        remote.type_text("Hi!", CommandId::new()).await.unwrap();

        let reports = remote.transport().recorded_reports();
        // 'H' (Press, Release), 'i' (Press, Release), '!' (Press, Release) => 6 reports
        assert_eq!(reports.len(), 6);
        // 'H' is keycode 0x0B with MOD_LEFT_SHIFT (0x02)
        assert_eq!(reports[0], HidKeyboardReport::key_press(0x0B, 0x02));
        assert_eq!(reports[1], HidKeyboardReport::release_all());
        // 'i' is keycode 0x0C with 0
        assert_eq!(reports[2], HidKeyboardReport::key_press(0x0C, 0));
        assert_eq!(reports[3], HidKeyboardReport::release_all());
        // '!' is keycode 0x1E with MOD_LEFT_SHIFT (0x02)
        assert_eq!(reports[4], HidKeyboardReport::key_press(0x1E, 0x02));
        assert_eq!(reports[5], HidKeyboardReport::release_all());
    }

    #[tokio::test]
    async fn test_device_handle_traits() {
        let mock = MockBluetoothTransport::new();
        let addr = BluetoothAddress::try_from("11:22:33:44:55:66").unwrap();
        let remote = BluetoothRemote::new(addr, mock);

        assert_eq!(remote.id().as_str(), "bluetooth-11:22:33:44:55:66");
        assert!(remote.capabilities().contains(Capability::Key));
        assert!(remote.capabilities().contains(Capability::TextInput));
        assert!(remote.as_power().is_some());
        assert!(remote.as_key().is_some());
        assert!(remote.as_text().is_some());
    }

    #[tokio::test]
    async fn test_bluetooth_remote_sleep() {
        let mock = MockBluetoothTransport::new();
        let addr = BluetoothAddress::try_from("AA:BB:CC:DD:EE:FF").unwrap();
        let remote =
            BluetoothRemote::new(addr, mock).with_key_press_duration(Duration::from_millis(1));

        remote.sleep().await.unwrap();

        let reports = remote.transport().recorded_reports();
        assert_eq!(reports.len(), 2);
        // Press Power (0x66)
        assert_eq!(reports[0], HidKeyboardReport::key_press(0x66, 0));
        assert_eq!(reports[1], HidKeyboardReport::release_all());
        // Disconnected after sleep
        assert!(!remote.transport().is_connected().await.unwrap());
    }
}
