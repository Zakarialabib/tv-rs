//! DDC/CI monitor control over `/dev/i2c` on Linux.

use std::path::Path;
use thiserror::Error;

/// DDC/CI I2C slave address (7-bit address).
pub const DDC_I2C_ADDR: u16 = 0x37;

/// DDC/CI host address.
pub const DDC_HOST_ADDR: u8 = 0x51;

/// VCP Feature Code 0x10: Luminance / Brightness.
pub const VCP_BRIGHTNESS: u8 = 0x10;

/// VCP Feature Code 0x60: Input Source Selection.
pub const VCP_INPUT_SOURCE: u8 = 0x60;

/// Errors arising from DDC/CI monitor control over I2C.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum DdcError {
    /// Insufficient permissions to access `/dev/i2c` device node.
    #[error("Permission denied accessing I2C device: {0}")]
    PermissionDenied(String),

    /// I/O or system call failure on the I2C bus.
    #[error("I2C bus error: {0}")]
    I2cError(String),

    /// DDC/CI protocol, checksum, or response parsing error.
    #[error("DDC/CI communication failed: {0}")]
    CommunicationFailed(String),

    /// The monitor reported that the requested VCP feature code is unsupported.
    #[error("Unsupported VCP feature code: 0x{0:02X}")]
    UnsupportedVcp(u8),
}

/// A parsed VCP response value (current and maximum).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VcpValue {
    /// Current feature value.
    pub current: u16,
    /// Maximum feature value reported by monitor.
    pub max: u16,
}

/// Builds a DDC/CI Get VCP Feature frame (`0x01`).
pub fn build_get_vcp_frame(vcp_code: u8) -> Vec<u8> {
    let mut frame = vec![DDC_HOST_ADDR, 0x82, 0x01, vcp_code];
    let checksum = 0x6E ^ DDC_HOST_ADDR ^ 0x82 ^ 0x01 ^ vcp_code;
    frame.push(checksum);
    frame
}

/// Builds a DDC/CI Set VCP Feature frame (`0x03`).
pub fn build_set_vcp_frame(vcp_code: u8, value: u16) -> Vec<u8> {
    let hi = (value >> 8) as u8;
    let lo = (value & 0xFF) as u8;
    let mut frame = vec![DDC_HOST_ADDR, 0x84, 0x03, vcp_code, hi, lo];
    let checksum = 0x6E ^ DDC_HOST_ADDR ^ 0x84 ^ 0x03 ^ vcp_code ^ hi ^ lo;
    frame.push(checksum);
    frame
}

/// Parses a DDC/CI Get VCP Feature reply.
pub fn parse_vcp_response(bytes: &[u8], expected_vcp: u8) -> Result<VcpValue, DdcError> {
    if bytes.len() < 11 {
        return Err(DdcError::CommunicationFailed(format!(
            "response too short: expected at least 11 bytes, got {}",
            bytes.len()
        )));
    }

    // Source address may be 0x6E or 0x50 depending on display firmware.
    let length_byte = bytes[1];
    let payload_len = (length_byte & 0x7F) as usize;

    if bytes.len() < 2 + payload_len {
        return Err(DdcError::CommunicationFailed(format!(
            "frame length mismatch: declared payload {payload_len}, total available {}",
            bytes.len()
        )));
    }

    let command = bytes[2];
    if command != 0x02 {
        return Err(DdcError::CommunicationFailed(format!(
            "unexpected command byte 0x{command:02X}; expected 0x02 VCP Reply"
        )));
    }

    let result_code = bytes[3];
    if result_code != 0x00 {
        return Err(DdcError::UnsupportedVcp(expected_vcp));
    }

    let vcp_code = bytes[4];
    if vcp_code != expected_vcp {
        return Err(DdcError::CommunicationFailed(format!(
            "reply VCP code 0x{vcp_code:02X} does not match expected 0x{expected_vcp:02X}"
        )));
    }

    let max_hi = bytes[6];
    let max_lo = bytes[7];
    let cur_hi = bytes[8];
    let cur_lo = bytes[9];

    let max = ((max_hi as u16) << 8) | (max_lo as u16);
    let current = ((cur_hi as u16) << 8) | (cur_lo as u16);

    Ok(VcpValue { current, max })
}

/// Handles DDC/CI VCP queries and commands for a Linux `/dev/i2c-*` monitor node.
pub struct DdcMonitor {
    #[allow(dead_code)]
    dev_path: String,
}

impl DdcMonitor {
    /// Connects to a Linux `/dev/i2c-N` device node.
    pub fn open(dev_path: impl AsRef<Path>) -> Result<Self, DdcError> {
        let path = dev_path.as_ref();
        if !path.exists() {
            return Err(DdcError::I2cError(format!(
                "device node `{}` does not exist",
                path.display()
            )));
        }

        // On Linux, verify read/write permission to the device node.
        match std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
        {
            Ok(_) => Ok(Self {
                dev_path: path.to_string_lossy().to_string(),
            }),
            Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => {
                Err(DdcError::PermissionDenied(format!(
                    "cannot open `{}`: permission denied",
                    path.display()
                )))
            }
            Err(err) => Err(DdcError::I2cError(err.to_string())),
        }
    }

    /// Reads a VCP feature value (e.g. brightness or input source).
    #[cfg(target_os = "linux")]
    pub fn get_vcp(&self, vcp_code: u8) -> Result<VcpValue, DdcError> {
        use std::os::unix::io::AsRawFd;

        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&self.dev_path)
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::PermissionDenied => DdcError::PermissionDenied(e.to_string()),
                _ => DdcError::I2cError(e.to_string()),
            })?;

        // I2C_SLAVE ioctl is 0x0703
        const I2C_SLAVE: libc::c_ulong = 0x0703;
        #[allow(unsafe_code)]
        let res =
            unsafe { libc::ioctl(file.as_raw_fd(), I2C_SLAVE, DDC_I2C_ADDR as libc::c_ulong) };
        if res < 0 {
            return Err(DdcError::I2cError(
                "failed to set I2C slave address".to_string(),
            ));
        }

        let frame = build_get_vcp_frame(vcp_code);
        use std::io::{Read, Write};
        let mut f = file;
        f.write_all(&frame)
            .map_err(|e| DdcError::I2cError(format!("I2C write failed: {e}")))?;

        // Give monitor a brief moment to compute response
        std::thread::sleep(std::time::Duration::from_millis(40));

        let mut buf = [0u8; 64];
        let n = f
            .read(&mut buf)
            .map_err(|e| DdcError::I2cError(format!("I2C read failed: {e}")))?;

        parse_vcp_response(&buf[..n], vcp_code)
    }

    /// Reads a VCP feature value on non-Linux platforms (stub fallback).
    #[cfg(not(target_os = "linux"))]
    pub fn get_vcp(&self, _vcp_code: u8) -> Result<VcpValue, DdcError> {
        Err(DdcError::I2cError(
            "DDC/CI over /dev/i2c is only supported on Linux".to_string(),
        ))
    }

    /// Sets a VCP feature value.
    #[cfg(target_os = "linux")]
    pub fn set_vcp(&self, vcp_code: u8, value: u16) -> Result<(), DdcError> {
        use std::os::unix::io::AsRawFd;

        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&self.dev_path)
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::PermissionDenied => DdcError::PermissionDenied(e.to_string()),
                _ => DdcError::I2cError(e.to_string()),
            })?;

        const I2C_SLAVE: libc::c_ulong = 0x0703;
        #[allow(unsafe_code)]
        let res =
            unsafe { libc::ioctl(file.as_raw_fd(), I2C_SLAVE, DDC_I2C_ADDR as libc::c_ulong) };
        if res < 0 {
            return Err(DdcError::I2cError(
                "failed to set I2C slave address".to_string(),
            ));
        }

        let frame = build_set_vcp_frame(vcp_code, value);
        use std::io::Write;
        let mut f = file;
        f.write_all(&frame)
            .map_err(|e| DdcError::I2cError(format!("I2C write failed: {e}")))?;

        Ok(())
    }

    /// Sets a VCP feature value on non-Linux platforms (stub fallback).
    #[cfg(not(target_os = "linux"))]
    pub fn set_vcp(&self, _vcp_code: u8, _value: u16) -> Result<(), DdcError> {
        Err(DdcError::I2cError(
            "DDC/CI over /dev/i2c is only supported on Linux".to_string(),
        ))
    }

    /// Convenience: read monitor brightness (VCP 0x10).
    pub fn brightness(&self) -> Result<u16, DdcError> {
        self.get_vcp(VCP_BRIGHTNESS).map(|v| v.current)
    }

    /// Convenience: set monitor brightness (VCP 0x10).
    pub fn set_brightness(&self, value: u16) -> Result<(), DdcError> {
        self.set_vcp(VCP_BRIGHTNESS, value)
    }

    /// Convenience: read active input source (VCP 0x60).
    pub fn input_source(&self) -> Result<u16, DdcError> {
        self.get_vcp(VCP_INPUT_SOURCE).map(|v| v.current)
    }

    /// Convenience: set active input source (VCP 0x60).
    pub fn set_input_source(&self, value: u16) -> Result<(), DdcError> {
        self.set_vcp(VCP_INPUT_SOURCE, value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_get_vcp_frame() {
        let frame = build_get_vcp_frame(VCP_BRIGHTNESS);
        assert_eq!(frame, vec![0x51, 0x82, 0x01, 0x10, 0xAC]);

        let frame_input = build_get_vcp_frame(VCP_INPUT_SOURCE);
        assert_eq!(frame_input, vec![0x51, 0x82, 0x01, 0x60, 0xDC]);
    }

    #[test]
    fn test_build_set_vcp_frame() {
        let frame = build_set_vcp_frame(VCP_BRIGHTNESS, 75);
        assert_eq!(frame, vec![0x51, 0x84, 0x03, 0x10, 0x00, 0x4B, 0xE3]);
    }

    #[test]
    fn test_parse_vcp_response_success() {
        // Valid Get VCP 0x10 reply: current=75, max=100
        let resp = vec![
            0x6E, 0x88, 0x02, 0x00, 0x10, 0x00, 0x00, 0x64, 0x00, 0x4B, 0x00,
        ];
        let val = parse_vcp_response(&resp, VCP_BRIGHTNESS).expect("parse valid response");
        assert_eq!(val.current, 75);
        assert_eq!(val.max, 100);
    }

    #[test]
    fn test_parse_vcp_response_unsupported() {
        let resp = vec![
            0x6E, 0x88, 0x02, 0x01, 0x10, 0x00, 0x00, 0x64, 0x00, 0x4B, 0x00,
        ];
        let err =
            parse_vcp_response(&resp, VCP_BRIGHTNESS).expect_err("result code 1 = unsupported");
        assert_eq!(err, DdcError::UnsupportedVcp(VCP_BRIGHTNESS));
    }

    #[test]
    fn test_parse_vcp_response_short_or_mismatched() {
        let short = vec![0x6E, 0x88, 0x02];
        assert!(matches!(
            parse_vcp_response(&short, VCP_BRIGHTNESS),
            Err(DdcError::CommunicationFailed(_))
        ));

        let mismatch_vcp = vec![
            0x6E, 0x88, 0x02, 0x00, 0x60, 0x00, 0x00, 0x64, 0x00, 0x4B, 0x00,
        ];
        assert!(matches!(
            parse_vcp_response(&mismatch_vcp, VCP_BRIGHTNESS),
            Err(DdcError::CommunicationFailed(_))
        ));
    }
}
