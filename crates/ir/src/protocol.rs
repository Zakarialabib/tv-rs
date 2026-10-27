//! Low-level IR protocol definitions, carrier frequencies, and pulse-timing encoders.

use crate::error::IrError;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Supported IR protocol standards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IrProtocol {
    /// NEC protocol (used by Samsung, LG, Amlogic TV boxes, etc.). 38 kHz.
    Nec,
    /// Philips RC5 protocol (biphase modulated). 36 kHz.
    Rc5,
    /// Philips RC6 protocol. 36 kHz.
    Rc6,
    /// Sony SIRC protocol (12-bit / 15-bit / 20-bit). 40 kHz.
    Sirc,
    /// RCA protocol. 38 kHz or 56 kHz.
    Rca,
    /// Amlogic-specific NEC variant (38 kHz).
    Amlogic,
}

impl IrProtocol {
    /// Returns the default carrier frequency for this protocol in kHz.
    #[must_use]
    pub fn default_carrier_khz(&self) -> u32 {
        match self {
            Self::Nec | Self::Amlogic => 38,
            Self::Rc5 | Self::Rc6 => 36,
            Self::Sirc => 40,
            Self::Rca => 38,
        }
    }

    /// Stable string identifier for logging and config.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Nec => "nec",
            Self::Rc5 => "rc5",
            Self::Rc6 => "rc6",
            Self::Sirc => "sirc",
            Self::Rca => "rca",
            Self::Amlogic => "amlogic",
        }
    }
}

impl fmt::Display for IrProtocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// An IR signal representation containing protocol type, carrier frequency, hex code payload, and microsecond pulse timings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IrSignal {
    /// IR protocol used.
    pub protocol: IrProtocol,
    /// Carrier frequency in kHz (typically 36, 38, 40, or 56).
    pub carrier_khz: u32,
    /// Device / customer address (if applicable).
    pub address: u16,
    /// Command payload code.
    pub command: u32,
    /// Raw microsecond timings alternating between mark (carrier ON) and space (carrier OFF).
    pub pulses: Vec<u32>,
}

impl IrSignal {
    /// Encodes an IR signal from protocol parameters.
    ///
    /// # Errors
    ///
    /// Returns [`IrError::Protocol`] if the protocol configuration or code values are invalid.
    pub fn encode(
        protocol: IrProtocol,
        address: u16,
        command: u32,
        carrier_khz: Option<u32>,
    ) -> Result<Self, IrError> {
        let freq = carrier_khz.unwrap_or_else(|| protocol.default_carrier_khz());
        let pulses = match protocol {
            IrProtocol::Nec | IrProtocol::Amlogic => encode_nec(address, command),
            IrProtocol::Rc5 => encode_rc5(address, command),
            IrProtocol::Rc6 => encode_rc6(address, command),
            IrProtocol::Sirc => encode_sirc(address, command),
            IrProtocol::Rca => encode_rca(address, command),
        }?;

        Ok(Self {
            protocol,
            carrier_khz: freq,
            address,
            command,
            pulses,
        })
    }

    /// Renders raw pulse sequence as a string of microsecond durations.
    #[must_use]
    pub fn to_raw_timing_string(&self) -> String {
        self.pulses
            .iter()
            .map(|p| p.to_string())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Encodes standard NEC / Amlogic frame pulse timings.
/// Leader: 9000µs mark, 4500µs space.
/// Logic 0: 560µs mark, 560µs space.
/// Logic 1: 560µs mark, 1690µs space.
/// End: 560µs mark.
fn encode_nec(address: u16, command: u32) -> Result<Vec<u32>, IrError> {
    let mut pulses = Vec::with_capacity(67);
    // Leader
    pulses.push(9000);
    pulses.push(4500);

    // 16-bit address (low byte + inverted high byte if address < 256, or full 16-bit address)
    let addr_payload: u16 = if address <= 0xFF {
        ((!address & 0xFF) << 8) | (address & 0xFF)
    } else {
        address
    };

    // 16-bit command (low byte command + inverted command byte)
    let cmd_byte = (command & 0xFF) as u8;
    let cmd_inv = !cmd_byte;
    let cmd_payload: u16 = ((cmd_inv as u16) << 8) | (cmd_byte as u16);

    let full_payload: u32 = ((cmd_payload as u32) << 16) | (addr_payload as u32);

    for i in 0..32 {
        let bit = (full_payload >> i) & 1;
        pulses.push(560);
        if bit == 1 {
            pulses.push(1690);
        } else {
            pulses.push(560);
        }
    }
    // Trailing stop bit mark
    pulses.push(560);

    Ok(pulses)
}

/// Encodes Sony SIRC (12-bit standard) pulse timings.
/// Header: 2400µs mark, 600µs space.
/// Logic 0: 600µs mark, 600µs space.
/// Logic 1: 1200µs mark, 600µs space.
fn encode_sirc(address: u16, command: u32) -> Result<Vec<u32>, IrError> {
    let mut pulses = Vec::with_capacity(26);
    // Header
    pulses.push(2400);
    pulses.push(600);

    // 7-bit command followed by 5-bit address
    let cmd_bits = command & 0x7F;
    let addr_bits = ((address as u32) & 0x1F) << 7;
    let payload = cmd_bits | addr_bits;

    for i in 0..12 {
        let bit = (payload >> i) & 1;
        if bit == 1 {
            pulses.push(1200);
        } else {
            pulses.push(600);
        }
        pulses.push(600);
    }

    Ok(pulses)
}

/// Encodes Philips RC5 pulse timings (14 bits total, 889µs bit width biphase).
fn encode_rc5(address: u16, command: u32) -> Result<Vec<u32>, IrError> {
    let mut pulses = Vec::new();
    let bit_period = 889u32;

    // RC5 payload: 2 start bits (1, 1), 1 toggle bit (0), 5 address bits, 6 command bits
    let payload: u16 = (1 << 13) | (1 << 12) | ((address & 0x1F) << 6) | ((command as u16) & 0x3F);

    let mut current_level = true; // Mark first part of start bit
    let mut current_duration = 0u32;

    for i in (0..14).rev() {
        let bit = (payload >> i) & 1;
        // RC5 biphase: 1 is (space -> mark), 0 is (mark -> space)
        let (first_half_mark, second_half_mark) = if bit == 1 {
            (false, true)
        } else {
            (true, false)
        };

        for half in [first_half_mark, second_half_mark] {
            if half == current_level {
                current_duration += bit_period;
            } else {
                if current_duration > 0 {
                    pulses.push(current_duration);
                }
                current_level = half;
                current_duration = bit_period;
            }
        }
    }
    if current_duration > 0 {
        pulses.push(current_duration);
    }

    Ok(pulses)
}

/// Encodes Philips RC6 Mode 0 pulse timings.
fn encode_rc6(address: u16, command: u32) -> Result<Vec<u32>, IrError> {
    let mut pulses = Vec::new();
    // Header: 2666µs mark, 889µs space
    pulses.push(2666);
    pulses.push(889);

    // Standard 16-bit payload (8-bit address, 8-bit command)
    let payload: u16 = ((address & 0xFF) << 8) | ((command as u16) & 0xFF);
    for i in (0..16).rev() {
        let bit = (payload >> i) & 1;
        if bit == 1 {
            pulses.push(444);
            pulses.push(444);
        } else {
            pulses.push(444);
            pulses.push(889);
        }
    }

    Ok(pulses)
}

/// Encodes RCA protocol pulse timings.
/// Header: 4000µs mark, 4000µs space.
/// Logic 0: 500µs mark, 1000µs space.
/// Logic 1: 500µs mark, 2000µs space.
fn encode_rca(address: u16, command: u32) -> Result<Vec<u32>, IrError> {
    let mut pulses = Vec::with_capacity(50);
    // Header
    pulses.push(4000);
    pulses.push(4000);

    let payload = (((address as u32) & 0x0F) << 8) | (command & 0xFF);
    for i in (0..12).rev() {
        let bit = (payload >> i) & 1;
        pulses.push(500);
        if bit == 1 {
            pulses.push(2000);
        } else {
            pulses.push(1000);
        }
    }
    pulses.push(500);

    Ok(pulses)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_nec_signal_correctly() {
        let sig = IrSignal::encode(IrProtocol::Nec, 0x07, 0x12, None).unwrap();
        assert_eq!(sig.protocol, IrProtocol::Nec);
        assert_eq!(sig.carrier_khz, 38);
        assert_eq!(sig.pulses[0], 9000);
        assert_eq!(sig.pulses[1], 4500);
        assert!(!sig.pulses.is_empty());
    }

    #[test]
    fn encodes_sirc_signal_correctly() {
        let sig = IrSignal::encode(IrProtocol::Sirc, 0x01, 0x15, None).unwrap();
        assert_eq!(sig.protocol, IrProtocol::Sirc);
        assert_eq!(sig.carrier_khz, 40);
        assert_eq!(sig.pulses[0], 2400);
        assert_eq!(sig.pulses[1], 600);
    }
}
