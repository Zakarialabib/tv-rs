//! VCP opcodes, value mappings, and DDC/CI packet framing.

use tv_rs_core::{Capability, Input, PowerState, TvError};

/// VCP (Virtual Control Panel) opcodes defined by VESA MCCS/DDC/CI standards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum VcpOpcode {
    /// Luminance / Brightness control (`0x10`).
    Brightness = 0x10,
    /// Active Input Source selection (`0x60`).
    InputSelect = 0x60,
    /// Display Power State (`0xD6`).
    PowerState = 0xD6,
    /// Custom or unmodeled VCP opcode.
    Custom(u8),
}

impl VcpOpcode {
    /// Returns the raw `u8` VCP opcode.
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::Brightness => 0x10,
            Self::InputSelect => 0x60,
            Self::PowerState => 0xD6,
            Self::Custom(c) => c,
        }
    }

    /// Converts a raw `u8` code into a [`VcpOpcode`].
    #[must_use]
    pub const fn from_code(code: u8) -> Self {
        match code {
            0x10 => Self::Brightness,
            0x60 => Self::InputSelect,
            0xD6 => Self::PowerState,
            other => Self::Custom(other),
        }
    }
}

/// Converts a VCP input selection value to a `tv_rs_core::Input`.
#[must_use]
pub fn vcp_value_to_input(value: u16) -> Input {
    match value {
        0x01 => Input::Other("vga1".into()),
        0x02 => Input::Other("vga2".into()),
        0x03 => Input::Other("dvi1".into()),
        0x04 => Input::Other("dvi2".into()),
        0x05 => Input::Other("composite1".into()),
        0x07 => Input::Other("svideo1".into()),
        0x09 => Input::Tv,
        0x0F => Input::Other("dp1".into()),
        0x10 => Input::Other("dp2".into()),
        0x11 => Input::Hdmi(1),
        0x12 => Input::Hdmi(2),
        0x13 => Input::Hdmi(3),
        0x14 => Input::Hdmi(4),
        0x1B => Input::Usb(1),
        other => Input::Other(format!("vcp_0x{other:02X}")),
    }
}

/// Converts a `tv_rs_core::Input` to its corresponding VCP input selection value.
///
/// # Errors
///
/// Returns [`TvError::Unsupported`] if the input cannot be represented in standard VCP codes.
pub fn input_to_vcp_value(input: &Input) -> Result<u16, TvError> {
    match input {
        Input::Hdmi(1) => Ok(0x11),
        Input::Hdmi(2) => Ok(0x12),
        Input::Hdmi(3) => Ok(0x13),
        Input::Hdmi(4) => Ok(0x14),
        Input::Hdmi(_) => Err(TvError::Unsupported(Capability::InputSelect)),
        Input::Tv => Ok(0x09),
        Input::Usb(1) => Ok(0x1B),
        Input::Usb(_) => Err(TvError::Unsupported(Capability::InputSelect)),
        Input::Other(s) => match s.to_lowercase().as_str() {
            "vga1" | "vga" | "analog1" => Ok(0x01),
            "vga2" | "analog2" => Ok(0x02),
            "dvi1" | "dvi" => Ok(0x03),
            "dvi2" => Ok(0x04),
            "dp1" | "dp" | "displayport1" | "displayport" => Ok(0x0F),
            "dp2" | "displayport2" => Ok(0x10),
            _ if s.starts_with("vcp_0x") => u16::from_str_radix(&s[6..], 16)
                .map_err(|_| TvError::Unsupported(Capability::InputSelect)),
            _ => Err(TvError::Unsupported(Capability::InputSelect)),
        },
    }
}

/// Converts a VCP power mode value (`VCP 0xD6`) to a `tv_rs_core::PowerState`.
#[must_use]
pub fn vcp_value_to_power_state(value: u16) -> PowerState {
    match value {
        0x01 => PowerState::On,
        0x02 => PowerState::Standby,
        0x03..=0x05 => PowerState::Off,
        _ => PowerState::Unknown,
    }
}

/// Converts a `tv_rs_core::PowerState` to a VCP power mode value (`VCP 0xD6`).
#[must_use]
pub const fn power_state_to_vcp_value(state: PowerState) -> u16 {
    match state {
        PowerState::On => 0x01,
        PowerState::Standby => 0x02,
        PowerState::Off => 0x04,
        PowerState::Unknown => 0x01,
    }
}

/// Standard DDC/CI I2C addresses.
pub const DDC_I2C_ADDR: u8 = 0x37;
/// DDC/CI destination byte on write.
pub const DDC_DEST_ADDR: u8 = 0x6E;
/// DDC/CI host source byte.
pub const DDC_SRC_ADDR: u8 = 0x51;
/// Virtual host address for calculating read response checksums.
pub const DDC_VIRTUAL_READ_ADDR: u8 = 0x50;

/// Encodes a DDC/CI Get VCP request frame.
///
/// Returns the full byte sequence to write over I2C to address `0x37`.
#[must_use]
pub fn encode_get_vcp_frame(opcode: VcpOpcode) -> Vec<u8> {
    let payload = [0x01, opcode.code()];
    encode_frame(&payload)
}

/// Encodes a DDC/CI Set VCP command frame.
///
/// Returns the full byte sequence to write over I2C to address `0x37`.
#[must_use]
pub fn encode_set_vcp_frame(opcode: VcpOpcode, value: u16) -> Vec<u8> {
    let val_hi = (value >> 8) as u8;
    let val_lo = (value & 0xFF) as u8;
    let payload = [0x03, opcode.code(), val_hi, val_lo];
    encode_frame(&payload)
}

/// Encodes a DDC/CI request frame from payload bytes.
fn encode_frame(payload: &[u8]) -> Vec<u8> {
    let len_byte = 0x80 | (payload.len() as u8);
    let mut frame = Vec::with_capacity(3 + payload.len() + 1);
    frame.push(DDC_DEST_ADDR);
    frame.push(DDC_SRC_ADDR);
    frame.push(len_byte);
    frame.extend_from_slice(payload);

    let mut checksum = 0u8;
    for &b in &frame {
        checksum ^= b;
    }
    frame.push(checksum);
    frame
}

/// Decodes a DDC/CI Get VCP reply frame.
///
/// Returns `(current_value, max_value)` on success.
///
/// # Errors
///
/// Returns [`TvError::Protocol`] if the frame is malformed, has checksum mismatch,
/// or reports a VCP error.
pub fn decode_vcp_reply(frame: &[u8], expected_opcode: VcpOpcode) -> Result<(u16, u16), TvError> {
    if frame.len() < 10 {
        return Err(TvError::protocol(format!(
            "DDC/CI reply frame too short: len {}",
            frame.len()
        )));
    }

    // Verify checksum.
    // Checksum = DDC_VIRTUAL_READ_ADDR ^ source ^ length ^ payload...
    let mut calculated_checksum = DDC_VIRTUAL_READ_ADDR;
    let frame_checksum = frame[frame.len() - 1];
    for &b in &frame[..frame.len() - 1] {
        calculated_checksum ^= b;
    }

    if calculated_checksum != frame_checksum {
        return Err(TvError::protocol(format!(
            "DDC/CI reply checksum mismatch: calculated 0x{calculated_checksum:02X}, got 0x{frame_checksum:02X}"
        )));
    }

    // Parse reply header
    // Byte 0: Source addr (0x6E)
    // Byte 1: Length byte (0x80 | payload_len)
    let payload_len = (frame[1] & 0x7F) as usize;
    if payload_len < 8 {
        return Err(TvError::protocol(format!(
            "DDC/CI reply payload length too short: {payload_len}"
        )));
    }
    if frame.len() < 2 + payload_len + 1 {
        return Err(TvError::protocol(format!(
            "DDC/CI reply frame truncated: frame len {}, required {}",
            frame.len(),
            2 + payload_len + 1
        )));
    }

    let payload = &frame[2..2 + payload_len];
    if payload[0] != 0x02 {
        return Err(TvError::protocol(format!(
            "Unexpected DDC/CI command reply byte: 0x{:02X} (expected 0x02)",
            payload[0]
        )));
    }

    let result_code = payload[1];
    if result_code != 0x00 {
        return Err(TvError::protocol(format!(
            "DDC/CI error result code: 0x{result_code:02X}"
        )));
    }

    let reply_opcode = payload[2];
    if reply_opcode != expected_opcode.code() {
        return Err(TvError::protocol(format!(
            "DDC/CI reply opcode mismatch: expected 0x{:02X}, got 0x{:02X}",
            expected_opcode.code(),
            reply_opcode
        )));
    }

    let max_hi = payload[4] as u16;
    let max_lo = payload[5] as u16;
    let cur_hi = payload[6] as u16;
    let cur_lo = payload[7] as u16;

    let max_val = (max_hi << 8) | max_lo;
    let cur_val = (cur_hi << 8) | cur_lo;

    Ok((cur_val, max_val))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vcp_opcodes_roundtrip() {
        assert_eq!(VcpOpcode::Brightness.code(), 0x10);
        assert_eq!(VcpOpcode::InputSelect.code(), 0x60);
        assert_eq!(VcpOpcode::PowerState.code(), 0xD6);
        assert_eq!(VcpOpcode::from_code(0x10), VcpOpcode::Brightness);
        assert_eq!(VcpOpcode::from_code(0x60), VcpOpcode::InputSelect);
        assert_eq!(VcpOpcode::from_code(0xD6), VcpOpcode::PowerState);
        assert_eq!(VcpOpcode::from_code(0x12), VcpOpcode::Custom(0x12));
    }

    #[test]
    fn input_conversions() {
        assert_eq!(vcp_value_to_input(0x11), Input::Hdmi(1));
        assert_eq!(vcp_value_to_input(0x12), Input::Hdmi(2));
        assert_eq!(vcp_value_to_input(0x09), Input::Tv);
        assert_eq!(vcp_value_to_input(0x1B), Input::Usb(1));

        assert_eq!(input_to_vcp_value(&Input::Hdmi(1)).unwrap(), 0x11);
        assert_eq!(input_to_vcp_value(&Input::Hdmi(2)).unwrap(), 0x12);
        assert_eq!(input_to_vcp_value(&Input::Tv).unwrap(), 0x09);
        assert_eq!(input_to_vcp_value(&Input::Usb(1)).unwrap(), 0x1B);
    }

    #[test]
    fn power_state_conversions() {
        assert_eq!(vcp_value_to_power_state(0x01), PowerState::On);
        assert_eq!(vcp_value_to_power_state(0x02), PowerState::Standby);
        assert_eq!(vcp_value_to_power_state(0x04), PowerState::Off);
        assert_eq!(power_state_to_vcp_value(PowerState::On), 0x01);
        assert_eq!(power_state_to_vcp_value(PowerState::Standby), 0x02);
        assert_eq!(power_state_to_vcp_value(PowerState::Off), 0x04);
    }

    #[test]
    fn encode_and_decode_get_vcp_frame() {
        let frame = encode_get_vcp_frame(VcpOpcode::Brightness);
        assert_eq!(frame[0], 0x6E); // Dest
        assert_eq!(frame[1], 0x51); // Src
        assert_eq!(frame[2], 0x82); // Len (0x80 | 2)
        assert_eq!(frame[3], 0x01); // Cmd Get VCP
        assert_eq!(frame[4], 0x10); // Opcode Brightness

        let expected_chk = 0x6E ^ 0x51 ^ 0x82 ^ 0x01 ^ 0x10;
        assert_eq!(frame[5], expected_chk);
    }

    #[test]
    fn encode_set_vcp_frame_test() {
        let frame = encode_set_vcp_frame(VcpOpcode::Brightness, 75);
        assert_eq!(frame[0], 0x6E);
        assert_eq!(frame[1], 0x51);
        assert_eq!(frame[2], 0x84); // Len (0x80 | 4)
        assert_eq!(frame[3], 0x03); // Cmd Set VCP
        assert_eq!(frame[4], 0x10); // Opcode
        assert_eq!(frame[5], 0x00); // Val HI
        assert_eq!(frame[6], 75); // Val LO
    }

    #[test]
    fn decode_valid_vcp_reply() {
        let mut reply_payload = vec![
            0x6E, // Source addr (monitor)
            0x88, // Length byte (0x80 | 8)
            0x02, // Command: Reply VCP
            0x00, // Result Code: OK
            0x10, // Opcode: Brightness
            0x00, // Type code: Set parameter
            0x00, 0x64, // Max value: 100
            0x00, 0x32, // Current value: 50
        ];

        let mut chk = DDC_VIRTUAL_READ_ADDR;
        for &b in &reply_payload {
            chk ^= b;
        }
        reply_payload.push(chk);

        let (cur, max) = decode_vcp_reply(&reply_payload, VcpOpcode::Brightness).unwrap();
        assert_eq!(cur, 50);
        assert_eq!(max, 100);
    }
}
