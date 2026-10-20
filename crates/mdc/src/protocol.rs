//! The MDC wire format: `0xAA`, command, display id, length, data, checksum.
//!
//! This module is pure and synchronous. It is the part to test first when a
//! panel rejects a command: a checksum error and a NAK are different problems,
//! and the byte layout makes that distinction explicit.

use crate::error::MdcError;
use crate::types::DisplayId;

/// Default TCP port for Samsung MDC over RJ45.
pub const MDC_TCP_PORT: u16 = 1515;

/// Every request starts with this byte.
pub const HEADER: u8 = 0xAA;

/// Every response uses `0xFF` in the command position.
pub const RESPONSE_COMMAND: u8 = 0xFF;

/// Acknowledgement byte.
pub const ACK: u8 = 0x41;

/// Negative acknowledgement byte.
pub const NAK: u8 = 0x4E;

/// Carriage return used by some TV firmware after a frame. The transport is
/// configured separately because commercial displays normally do not require it.
pub const TERMINATOR: u8 = 0x0D;

/// Command bytes used by this crate.
pub mod commands {
    /// Aggregated power, volume, mute, input and aspect status (query only).
    pub const STATUS: u8 = 0x00;
    /// Power (`0x11`).
    pub const POWER: u8 = 0x11;
    /// Volume (`0x12`).
    pub const VOLUME: u8 = 0x12;
    /// Mute (`0x13`).
    pub const MUTE: u8 = 0x13;
    /// Input source (`0x14`).
    pub const INPUT: u8 = 0x14;
    /// Brightness (`0x25`).
    pub const BRIGHTNESS: u8 = 0x25;
}

/// A request frame before it reaches the socket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// Command byte.
    pub command: u8,
    /// Target display.
    pub display_id: DisplayId,
    /// Command payload.
    pub data: Vec<u8>,
}

impl Frame {
    /// Builds a frame.
    #[must_use]
    pub fn new(command: u8, display_id: DisplayId, data: Vec<u8>) -> Self {
        Self {
            command,
            display_id,
            data,
        }
    }

    /// The checksum: the low byte of the sum of command, display id, length and data.
    #[must_use]
    pub fn checksum(&self) -> u8 {
        let mut sum = u16::from(self.command) + u16::from(self.display_id.value());
        sum += self.data.len() as u16;
        for byte in &self.data {
            sum += u16::from(*byte);
        }
        (sum & 0xFF) as u8
    }

    /// Encodes the frame, including the `0xAA` header and checksum.
    ///
    /// The optional carriage return is a transport concern and is not part of
    /// this byte sequence.
    ///
    /// # Errors
    ///
    /// Returns [`MdcError::FrameTooLong`] when the payload does not fit the
    /// one-byte MDC length field.
    pub fn encode(&self) -> Result<Vec<u8>, MdcError> {
        let length =
            u8::try_from(self.data.len()).map_err(|_| MdcError::FrameTooLong(self.data.len()))?;
        let mut bytes = Vec::with_capacity(5 + self.data.len());
        bytes.push(HEADER);
        bytes.push(self.command);
        bytes.push(self.display_id.value());
        bytes.push(length);
        bytes.extend_from_slice(&self.data);
        bytes.push(self.checksum());
        Ok(bytes)
    }
}

/// A parsed response frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    /// The display that answered.
    pub display_id: DisplayId,
    /// The original command echoed by the display.
    pub command: u8,
    /// Whether the display returned `ACK` rather than `NAK`.
    pub acknowledged: bool,
    /// `ACK` payload, or the single error byte for a `NAK`.
    pub data: Vec<u8>,
}

impl Response {
    /// Parses a complete response, including header and checksum.
    ///
    /// # Errors
    ///
    /// Returns [`MdcError::Protocol`] for a truncated frame, a bad header,
    /// a checksum mismatch, a malformed acknowledgement or a response whose
    /// body cannot be interpreted. It does not turn a `NAK` into an error;
    /// callers decide how to route a rejection.
    pub fn parse(bytes: &[u8]) -> Result<Self, MdcError> {
        if bytes.len() < 6 {
            return Err(MdcError::Protocol(format!(
                "response is {} bytes; a frame needs at least 6",
                bytes.len()
            )));
        }
        if bytes[0] != HEADER {
            return Err(MdcError::Protocol(format!(
                "unexpected header 0x{:02X}",
                bytes[0]
            )));
        }
        if bytes[1] != RESPONSE_COMMAND {
            return Err(MdcError::Protocol(format!(
                "unexpected response command 0x{:02X}",
                bytes[1]
            )));
        }

        let display_id = DisplayId::new(bytes[2])?;
        let length = usize::from(bytes[3]);
        if bytes.len() != 4 + length + 1 {
            return Err(MdcError::Protocol(format!(
                "response declares {length} data bytes but carries {}",
                bytes.len().saturating_sub(5)
            )));
        }

        let checksum = bytes[4 + length];
        let actual = bytes[1..4 + length]
            .iter()
            .fold(0u16, |sum, byte| sum + u16::from(*byte))
            & 0xFF;
        if checksum != actual as u8 {
            return Err(MdcError::Protocol(format!(
                "checksum 0x{checksum:02X} does not match 0x{actual:02X}"
            )));
        }

        let body = &bytes[4..4 + length];
        if body.len() < 2 {
            return Err(MdcError::Protocol(
                "response body is missing ACK/NAK and command".to_string(),
            ));
        }

        let acknowledged = match body[0] {
            ACK => true,
            NAK => false,
            other => {
                return Err(MdcError::Protocol(format!(
                    "unexpected ACK/NAK byte 0x{other:02X}"
                )))
            }
        };
        let data = body[2..].to_vec();
        if !acknowledged && data.len() != 1 {
            return Err(MdcError::Protocol(format!(
                "NAK carries {} data bytes; expected one error code",
                data.len()
            )));
        }

        Ok(Self {
            display_id,
            command: body[1],
            acknowledged,
            data,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{commands, Frame, Response, HEADER, RESPONSE_COMMAND};
    use crate::types::DisplayId;

    fn response(display_id: u8, ack: bool, command: u8, data: &[u8]) -> Vec<u8> {
        let ack_byte = if ack { super::ACK } else { super::NAK };
        let mut body = vec![ack_byte, command];
        body.extend_from_slice(data);
        let mut frame = vec![HEADER, RESPONSE_COMMAND, display_id, body.len() as u8];
        frame.extend_from_slice(&body);
        let checksum = frame[1..]
            .iter()
            .fold(0u16, |sum, byte| sum + u16::from(*byte))
            & 0xFF;
        frame.push(checksum as u8);
        frame
    }

    #[test]
    fn documented_power_frames_encode_exactly() {
        let on = Frame::new(commands::POWER, DisplayId::DIRECT, vec![0x01])
            .encode()
            .unwrap();
        assert_eq!(on, vec![0xAA, 0x11, 0x00, 0x01, 0x01, 0x13]);

        let off = Frame::new(commands::POWER, DisplayId::DIRECT, vec![0x00])
            .encode()
            .unwrap();
        assert_eq!(off, vec![0xAA, 0x11, 0x00, 0x01, 0x00, 0x12]);
    }

    #[test]
    fn documented_volume_and_brightness_frames_encode_exactly() {
        let volume = Frame::new(commands::VOLUME, DisplayId::DIRECT, vec![50])
            .encode()
            .unwrap();
        assert_eq!(volume, vec![0xAA, 0x12, 0x00, 0x01, 0x32, 0x45]);

        let brightness = Frame::new(commands::BRIGHTNESS, DisplayId::DIRECT, vec![50])
            .encode()
            .unwrap();
        assert_eq!(brightness, vec![0xAA, 0x25, 0x00, 0x01, 0x32, 0x58]);
    }

    #[test]
    fn display_id_is_part_of_the_checksum() {
        let frame = Frame::new(commands::POWER, DisplayId::new(1).unwrap(), vec![1])
            .encode()
            .unwrap();
        assert_eq!(frame, vec![0xAA, 0x11, 0x01, 0x01, 0x01, 0x14]);
    }

    #[test]
    fn ack_status_response_is_parsed() {
        let bytes = response(0x00, true, commands::STATUS, &[1, 50, 0, 0x21, 1, 0, 0]);
        let parsed = Response::parse(&bytes).unwrap();
        assert_eq!(parsed.display_id, DisplayId::DIRECT);
        assert_eq!(parsed.command, commands::STATUS);
        assert!(parsed.acknowledged);
        assert_eq!(parsed.data, vec![1, 50, 0, 0x21, 1, 0, 0]);
    }

    #[test]
    fn nak_preserves_the_error_code() {
        let bytes = response(0x01, false, commands::POWER, &[0x02]);
        let parsed = Response::parse(&bytes).unwrap();
        assert_eq!(parsed.display_id, DisplayId::new(1).unwrap());
        assert!(!parsed.acknowledged);
        assert_eq!(parsed.data, vec![0x02]);
    }

    #[test]
    fn a_bad_checksum_is_rejected() {
        let mut bytes = response(0x00, true, commands::POWER, &[1]);
        *bytes.last_mut().unwrap() ^= 0xFF;
        assert!(Response::parse(&bytes).is_err());
    }

    #[test]
    fn an_oversized_payload_is_rejected_before_it_reaches_a_socket() {
        let frame = Frame::new(commands::POWER, DisplayId::DIRECT, vec![0; 256]);
        assert!(matches!(
            frame.encode(),
            Err(crate::error::MdcError::FrameTooLong(256))
        ));
    }
}
