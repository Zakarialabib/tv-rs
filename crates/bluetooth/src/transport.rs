//! Bluetooth transport trait, CLI transport implementation (`bluetoothctl`), and mock transport.

use std::fmt;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use async_trait::async_trait;
use tokio::process::Command;

use crate::error::BluetoothError;
use crate::hid::HidKeyboardReport;

/// Bluetooth device address (BD_ADDR / MAC address).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BluetoothAddress(pub [u8; 6]);

impl BluetoothAddress {
    /// Formats the address as `AA:BB:CC:DD:EE:FF`.
    #[must_use]
    pub fn as_str(&self) -> String {
        format!(
            "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
            self.0[0], self.0[1], self.0[2], self.0[3], self.0[4], self.0[5]
        )
    }
}

impl fmt::Display for BluetoothAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.as_str())
    }
}

impl TryFrom<&str> for BluetoothAddress {
    type Error = BluetoothError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        let clean = s.trim().replace([':', '-'], "");
        if clean.len() != 12 {
            return Err(BluetoothError::InvalidAddress(s.to_string()));
        }
        let mut bytes = [0u8; 6];
        for i in 0..6 {
            bytes[i] = u8::from_str_radix(&clean[i * 2..i * 2 + 2], 16)
                .map_err(|_| BluetoothError::InvalidAddress(s.to_string()))?;
        }
        Ok(Self(bytes))
    }
}

/// Abstract transport layer for transmitting Bluetooth HID keyboard reports.
#[async_trait]
pub trait BluetoothTransport: Send + Sync {
    /// Connects to the target Bluetooth device.
    async fn connect(&self) -> Result<(), BluetoothError>;

    /// Disconnects from the target Bluetooth device.
    async fn disconnect(&self) -> Result<(), BluetoothError>;

    /// Sends one raw HID keyboard report.
    async fn send_report(&self, report: &HidKeyboardReport) -> Result<(), BluetoothError>;

    /// Checks if currently connected.
    async fn is_connected(&self) -> Result<bool, BluetoothError>;
}

/// A Bluetooth transport that communicates with BlueZ using the system `bluetoothctl` utility.
#[derive(Debug, Clone)]
pub struct BluetoothCtlTransport {
    address: BluetoothAddress,
    binary_path: String,
}

impl BluetoothCtlTransport {
    /// Creates a new `BluetoothCtlTransport` targeting `address`.
    #[must_use]
    pub fn new(address: BluetoothAddress) -> Self {
        Self {
            address,
            binary_path: "bluetoothctl".to_string(),
        }
    }

    /// Sets a custom path to the `bluetoothctl` executable.
    #[must_use]
    pub fn with_binary_path(mut self, path: impl Into<String>) -> Self {
        self.binary_path = path.into();
        self
    }
}

#[async_trait]
impl BluetoothTransport for BluetoothCtlTransport {
    async fn connect(&self) -> Result<(), BluetoothError> {
        let output = Command::new(&self.binary_path)
            .args(["connect", &self.address.as_str()])
            .stdin(Stdio::null())
            .output()
            .await
            .map_err(|err| {
                BluetoothError::AdapterUnavailable(format!(
                    "cannot execute '{}': {err}",
                    self.binary_path
                ))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(BluetoothError::ConnectionFailed(stderr.trim().to_string()));
        }
        Ok(())
    }

    async fn disconnect(&self) -> Result<(), BluetoothError> {
        let output = Command::new(&self.binary_path)
            .args(["disconnect", &self.address.as_str()])
            .stdin(Stdio::null())
            .output()
            .await
            .map_err(|err| {
                BluetoothError::AdapterUnavailable(format!(
                    "cannot execute '{}': {err}",
                    self.binary_path
                ))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(BluetoothError::ConnectionFailed(stderr.trim().to_string()));
        }
        Ok(())
    }

    async fn is_connected(&self) -> Result<bool, BluetoothError> {
        let output = Command::new(&self.binary_path)
            .args(["info", &self.address.as_str()])
            .stdin(Stdio::null())
            .output()
            .await
            .map_err(|err| {
                BluetoothError::AdapterUnavailable(format!(
                    "cannot execute '{}': {err}",
                    self.binary_path
                ))
            })?;

        if !output.status.success() {
            return Ok(false);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.contains("Connected: yes"))
    }

    async fn send_report(&self, report: &HidKeyboardReport) -> Result<(), BluetoothError> {
        if !self.is_connected().await? {
            return Err(BluetoothError::Disconnected);
        }

        let hex_bytes = hex::encode(report.to_bytes());
        let output = Command::new(&self.binary_path)
            .args([
                "menu",
                "gatt",
                "select-attribute",
                "report",
                "write",
                &hex_bytes,
            ])
            .stdin(Stdio::null())
            .output()
            .await
            .map_err(|err| {
                BluetoothError::AdapterUnavailable(format!(
                    "cannot execute '{}': {err}",
                    self.binary_path
                ))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(BluetoothError::TransmitFailed(stderr.trim().to_string()));
        }
        Ok(())
    }
}

/// A mock Bluetooth transport for testing without hardware.
#[derive(Debug, Default)]
pub struct MockBluetoothTransport {
    connected: AtomicBool,
    recorded_reports: Mutex<Vec<HidKeyboardReport>>,
    fail_on_connect: AtomicBool,
}

impl MockBluetoothTransport {
    /// Creates a new mock transport.
    #[must_use]
    pub fn new() -> Self {
        Self {
            connected: AtomicBool::new(false),
            recorded_reports: Mutex::new(Vec::new()),
            fail_on_connect: AtomicBool::new(false),
        }
    }

    /// Configures whether connect attempts should fail.
    pub fn set_fail_on_connect(&self, fail: bool) {
        self.fail_on_connect.store(fail, Ordering::SeqCst);
    }

    /// Returns all recorded HID reports sent through this mock.
    pub fn recorded_reports(&self) -> Vec<HidKeyboardReport> {
        self.recorded_reports.lock().unwrap().clone()
    }

    /// Clears the recorded reports.
    pub fn clear(&self) {
        self.recorded_reports.lock().unwrap().clear();
    }
}

#[async_trait]
impl BluetoothTransport for MockBluetoothTransport {
    async fn connect(&self) -> Result<(), BluetoothError> {
        if self.fail_on_connect.load(Ordering::SeqCst) {
            return Err(BluetoothError::ConnectionFailed(
                "mock connection failure".to_string(),
            ));
        }
        self.connected.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn disconnect(&self) -> Result<(), BluetoothError> {
        self.connected.store(false, Ordering::SeqCst);
        Ok(())
    }

    async fn send_report(&self, report: &HidKeyboardReport) -> Result<(), BluetoothError> {
        if !self.connected.load(Ordering::SeqCst) {
            return Err(BluetoothError::Disconnected);
        }
        self.recorded_reports.lock().unwrap().push(*report);
        Ok(())
    }

    async fn is_connected(&self) -> Result<bool, BluetoothError> {
        Ok(self.connected.load(Ordering::SeqCst))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bluetooth_address_parsing() {
        let addr = BluetoothAddress::try_from("AA:BB:CC:DD:EE:FF").unwrap();
        assert_eq!(addr.0, [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);
        assert_eq!(addr.as_str(), "AA:BB:CC:DD:EE:FF");

        let addr2 = BluetoothAddress::try_from("11-22-33-44-55-66").unwrap();
        assert_eq!(addr2.as_str(), "11:22:33:44:55:66");

        assert!(BluetoothAddress::try_from("invalid").is_err());
    }

    #[tokio::test]
    async fn test_mock_transport() {
        let mock = MockBluetoothTransport::new();
        assert!(!mock.is_connected().await.unwrap());

        // Report while disconnected fails
        let report = HidKeyboardReport::key_press(0x04, 0);
        assert!(mock.send_report(&report).await.is_err());

        // Connect and send report
        mock.connect().await.unwrap();
        assert!(mock.is_connected().await.unwrap());
        mock.send_report(&report).await.unwrap();

        assert_eq!(mock.recorded_reports().len(), 1);
        assert_eq!(mock.recorded_reports()[0], report);

        mock.disconnect().await.unwrap();
        assert!(!mock.is_connected().await.unwrap());
    }

    #[tokio::test]
    async fn test_bluetoothctl_transport_missing_binary() {
        let addr = BluetoothAddress::try_from("AA:BB:CC:DD:EE:FF").unwrap();
        let ctl =
            BluetoothCtlTransport::new(addr).with_binary_path("non_existent_bluetoothctl_bin");

        let result = ctl.connect().await;
        assert!(matches!(result, Err(BluetoothError::AdapterUnavailable(_))));
    }
}
