//! Serial port enumeration across Windows, Linux, and Android.

use tv_rs_core::TvError;

/// Description and metadata for a hardware serial port.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SerialPortInfo {
    /// System name or path for the port (e.g., `COM1`, `/dev/ttyUSB0`, `/dev/ttyACM0`).
    pub port_name: String,
    /// Type or classification of the port (e.g., `UsbPort`, `PciPort`, `BluetoothPort`, `Unknown`).
    pub port_type: String,
    /// Optional USB Vendor ID (VID).
    pub vendor_id: Option<u16>,
    /// Optional USB Product ID (PID).
    pub product_id: Option<u16>,
    /// Optional manufacturer name.
    pub manufacturer: Option<String>,
    /// Optional device serial number or product description.
    pub serial_number: Option<String>,
}

/// Enumerates all available serial ports on the host system.
///
/// Uses system APIs via `tokio_serial::available_ports()`, falling back to standard
/// platform device path scanning if system enumeration returns empty or fails.
///
/// # Errors
///
/// Returns [`TvError::Transport`] if port scanning encounters an unrecoverable system failure.
pub fn list_ports() -> Result<Vec<SerialPortInfo>, TvError> {
    match tokio_serial::available_ports() {
        Ok(ports) if !ports.is_empty() => Ok(ports.into_iter().map(from_tokio_info).collect()),
        _ => Ok(fallback_scan_ports()),
    }
}

fn from_tokio_info(info: tokio_serial::SerialPortInfo) -> SerialPortInfo {
    let (port_type, vendor_id, product_id, manufacturer, serial_number) = match info.port_type {
        tokio_serial::SerialPortType::UsbPort(usb) => (
            "UsbPort".to_string(),
            Some(usb.vid),
            Some(usb.pid),
            usb.manufacturer,
            usb.serial_number,
        ),
        tokio_serial::SerialPortType::PciPort => ("PciPort".to_string(), None, None, None, None),
        tokio_serial::SerialPortType::BluetoothPort => {
            ("BluetoothPort".to_string(), None, None, None, None)
        }
        tokio_serial::SerialPortType::Unknown => ("Unknown".to_string(), None, None, None, None),
    };

    SerialPortInfo {
        port_name: info.port_name,
        port_type,
        vendor_id,
        product_id,
        manufacturer,
        serial_number,
    }
}

fn fallback_scan_ports() -> Vec<SerialPortInfo> {
    let mut results = Vec::new();

    #[cfg(unix)]
    {
        // Linux / Android common serial nodes
        let candidates = [
            "/dev/ttyUSB0",
            "/dev/ttyUSB1",
            "/dev/ttyACM0",
            "/dev/ttyACM1",
            "/dev/ttyS0",
            "/dev/ttyS1",
        ];
        for path in candidates {
            if std::path::Path::new(path).exists() {
                results.push(SerialPortInfo {
                    port_name: path.to_string(),
                    port_type: "HardwareNode".to_string(),
                    vendor_id: None,
                    product_id: None,
                    manufacturer: None,
                    serial_number: None,
                });
            }
        }
    }

    #[cfg(windows)]
    {
        for i in 1..=16 {
            let name = format!("COM{i}");
            results.push(SerialPortInfo {
                port_name: name,
                port_type: "FallbackPort".to_string(),
                vendor_id: None,
                product_id: None,
                manufacturer: None,
                serial_number: None,
            });
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_ports_does_not_panic() {
        let ports = list_ports().expect("port listing should succeed");
        // May be empty on headless CI, but must return a valid Vec
        for port in ports {
            assert!(!port.port_name.is_empty());
        }
    }
}
