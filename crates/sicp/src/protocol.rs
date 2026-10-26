//! Philips SICP wire protocol framing and serialization.

use crate::error::SicpError;
use crate::types::DisplayId;

/// Default TCP port for Philips SICP display control.
pub const SICP_TCP_PORT: u16 = 5000;

/// SICP control message types.
pub mod control_types {
    /// SET command request (`0x00`).
    pub const SET: u8 = 0x00;
    /// GET status query (`0x01`).
    pub const GET: u8 = 0x01;
    /// REPLY / ACK response from display (`0x02`).
    pub const REPLY: u8 = 0x02;
    /// NAK error response from display (`0x03`).
    pub const NAK: u8 = 0x03;
}

/// SICP command bytes.
pub mod commands {
    /// Power status and control (`0x18`).
    pub const POWER: u8 = 0x18;
    /// Input source switch (`0x1A`).
    pub const INPUT: u8 = 0x1A;
    /// Volume control (`0x15`).
    pub const VOLUME: u8 = 0x15;
    /// Display temperature and health status (`0x2F`).
    pub const TEMPERATURE_STATUS: u8 = 0x2F;
}

/// A SICP request frame before serialization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// Target display ID.
    pub display_id: DisplayId,
    /// Control message type (SET, GET, REPLY, NAK).
    pub control_type: u8,
    /// Command code.
    pub command: u8,
    /// Command payload bytes.
    pub data: Vec<u8>,
}

impl Frame {
    /// Builds a new SICP frame.
    #[must_use]
    pub fn new(display_id: DisplayId, control_type: u8, command: u8, data: Vec<u8>) -> Self {
        Self {
            display_id,
            control_type,
            command,
            data,
        }
    }

    /// Calculates the XOR checksum over all preceding bytes in the frame.
    #[must_use]
    pub fn checksum(&self) -> u8 {
        let size = (5 + self.data.len()) as u8;
        let mut crc = size ^ self.display_id.value() ^ self.control_type ^ self.command;
        for byte in &self.data {
            crc ^= byte;
        }
        crc
    }

    /// Encodes the frame into a byte buffer ready for the wire.
    ///
    /// # Errors
    ///
    /// Returns [`SicpError::FrameTooLong`] if the data size exceeds 250 bytes.
    pub fn encode(&self) -> Result<Vec<u8>, SicpError> {
        let total_size = 5 + self.data.len();
        if total_size > 255 {
            return Err(SicpError::FrameTooLong(self.data.len()));
        }

        let mut bytes = Vec::with_capacity(total_size);
        bytes.push(total_size as u8);
        bytes.push(self.display_id.value());
        bytes.push(self.control_type);
        bytes.push(self.command);
        bytes.extend_from_slice(&self.data);
        bytes.push(self.checksum());
        Ok(bytes)
    }
}

/// A parsed SICP response frame from a display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    /// Display ID that sent the response.
    pub display_id: DisplayId,
    /// Control type byte (REPLY `0x02` or NAK `0x03`).
    pub control_type: u8,
    /// Command byte echoed by the display.
    pub command: u8,
    /// True if the message type is REPLY (`0x02`), false if NAK (`0x03`).
    pub acknowledged: bool,
    /// Response payload.
    pub data: Vec<u8>,
}

impl Response {
    /// Parses a complete SICP response frame from raw bytes.
    ///
    /// # Errors
    ///
    /// Returns [`SicpError::Protocol`] if the frame is truncated, checksum fails, or control type is unexpected.
    pub fn parse(bytes: &[u8]) -> Result<Self, SicpError> {
        if bytes.len() < 5 {
            return Err(SicpError::Protocol(format!(
                "frame is {} bytes; minimum SICP frame length is 5",
                bytes.len()
            )));
        }

        let declared_size = bytes[0] as usize;
        if bytes.len() != declared_size {
            return Err(SicpError::Protocol(format!(
                "frame declares size {declared_size} but actual length is {}",
                bytes.len()
            )));
        }

        let display_id = DisplayId::new(bytes[1])?;
        let control_type = bytes[2];
        let command = bytes[3];
        let checksum = bytes[bytes.len() - 1];

        let actual_checksum = bytes[..bytes.len() - 1]
            .iter()
            .fold(0u8, |acc, &byte| acc ^ byte);
        if checksum != actual_checksum {
            return Err(SicpError::Protocol(format!(
                "checksum 0x{checksum:02X} does not match calculated 0x{actual_checksum:02X}"
            )));
        }

        let acknowledged = match control_type {
            control_types::REPLY => true,
            control_types::NAK => false,
            other => {
                return Err(SicpError::Protocol(format!(
                    "unexpected control type byte 0x{other:02X}"
                )))
            }
        };

        let data = bytes[4..bytes.len() - 1].to_vec();

        Ok(Self {
            display_id,
            control_type,
            command,
            acknowledged,
            data,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_encoding_and_checksum() {
        let frame = Frame::new(
            DisplayId::DIRECT,
            control_types::GET,
            commands::POWER,
            vec![],
        );
        let encoded = frame.encode().unwrap();
        // size = 5, display_id = 1, control = 1, command = 0x18, checksum = 5^1^1^0x18 = 0x1D
        assert_eq!(encoded, vec![5, 1, 1, 0x18, 5 ^ 1 ^ 1 ^ 0x18]);

        let parsed = Response::parse(&[
            5,
            1,
            control_types::REPLY,
            commands::POWER,
            5 ^ 1 ^ 2 ^ 0x18,
        ])
        .unwrap();
        assert_eq!(parsed.display_id, DisplayId::DIRECT);
        assert_eq!(parsed.command, commands::POWER);
        assert!(parsed.acknowledged);
        assert!(parsed.data.is_empty());
    }

    #[test]
    fn bad_checksum_rejected() {
        let bytes = vec![5, 1, control_types::REPLY, commands::POWER, 0x00];
        assert!(Response::parse(&bytes).is_err());
    }

    #[test]
    fn nak_response_parsed() {
        let bytes = vec![
            6,
            1,
            control_types::NAK,
            commands::POWER,
            0x01,
            6 ^ 1 ^ 3 ^ 0x18 ^ 1,
        ];
        let parsed = Response::parse(&bytes).unwrap();
        assert!(!parsed.acknowledged);
        assert_eq!(parsed.data, vec![0x01]);
    }
}
