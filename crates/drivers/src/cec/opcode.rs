//! Standard HDMI-CEC opcodes, User Control codes, and frame encoders.

use tv_rs_core::capability::{Input, Key};
use tv_rs_core::descriptor::PowerState;
use tv_rs_core::error::TvError;

// Core CEC Opcodes
/// `<Image View On>` opcode.
pub const CEC_OPCODE_IMAGE_VIEW_ON: u8 = 0x04;
/// `<Text View On>` opcode.
pub const CEC_OPCODE_TEXT_VIEW_ON: u8 = 0x0D;
/// `<Standby>` opcode.
pub const CEC_OPCODE_STANDBY: u8 = 0x36;
/// `<User Control Pressed>` opcode.
pub const CEC_OPCODE_USER_CONTROL_PRESSED: u8 = 0x44;
/// `<User Control Released>` opcode.
pub const CEC_OPCODE_USER_CONTROL_RELEASED: u8 = 0x45;
/// `<Set Osd Name>` opcode.
pub const CEC_OPCODE_SET_OSD_NAME: u8 = 0x47;
/// `<Active Source>` opcode.
pub const CEC_OPCODE_ACTIVE_SOURCE: u8 = 0x82;
/// `<Request Active Source>` opcode.
pub const CEC_OPCODE_REQUEST_ACTIVE_SOURCE: u8 = 0x85;
/// `<Give Device Power Status>` opcode.
pub const CEC_OPCODE_GIVE_DEVICE_POWER_STATUS: u8 = 0x8F;
/// `<Report Power Status>` opcode.
pub const CEC_OPCODE_REPORT_POWER_STATUS: u8 = 0x90;

// User Control UI Codes (CEC Table 30)
/// Select UI code.
pub const CEC_USER_CONTROL_SELECT: u8 = 0x00;
/// Up UI code.
pub const CEC_USER_CONTROL_UP: u8 = 0x01;
/// Down UI code.
pub const CEC_USER_CONTROL_DOWN: u8 = 0x02;
/// Left UI code.
pub const CEC_USER_CONTROL_LEFT: u8 = 0x03;
/// Right UI code.
pub const CEC_USER_CONTROL_RIGHT: u8 = 0x04;
/// Root Menu / Home UI code.
pub const CEC_USER_CONTROL_ROOT_MENU: u8 = 0x09;
/// Exit / Back UI code.
pub const CEC_USER_CONTROL_EXIT: u8 = 0x0D;
/// Power UI code.
pub const CEC_USER_CONTROL_POWER: u8 = 0x40;
/// Volume Up UI code.
pub const CEC_USER_CONTROL_VOLUME_UP: u8 = 0x41;
/// Volume Down UI code.
pub const CEC_USER_CONTROL_VOLUME_DOWN: u8 = 0x42;
/// Mute UI code.
pub const CEC_USER_CONTROL_MUTE: u8 = 0x43;
/// Play UI code.
pub const CEC_USER_CONTROL_PLAY: u8 = 0x44;
/// Stop UI code.
pub const CEC_USER_CONTROL_STOP: u8 = 0x45;
/// Pause UI code.
pub const CEC_USER_CONTROL_PAUSE: u8 = 0x46;
/// Rewind UI code.
pub const CEC_USER_CONTROL_REWIND: u8 = 0x48;
/// Fast Forward UI code.
pub const CEC_USER_CONTROL_FAST_FORWARD: u8 = 0x49;
/// Power Off UI code.
pub const CEC_USER_CONTROL_POWER_OFF: u8 = 0x6C;

/// Formats a raw hex frame string for `cec-client` transmission.
#[must_use]
pub fn format_frame(initiator: u8, destination: u8, opcode: u8, params: &[u8]) -> String {
    let header = ((initiator & 0x0F) << 4) | (destination & 0x0F);
    if params.is_empty() {
        format!("tx {header:02X}:{opcode:02X}")
    } else {
        let mut hex = format!("tx {header:02X}:{opcode:02X}");
        for byte in params {
            hex.push_str(&format!(":{byte:02X}"));
        }
        hex
    }
}

/// Generates `<Image View On>` (opcode 0x04) frame from initiator to target.
#[must_use]
pub fn image_view_on_frame(initiator: u8, destination: u8) -> String {
    format_frame(initiator, destination, CEC_OPCODE_IMAGE_VIEW_ON, &[])
}

/// Generates `<Standby>` (opcode 0x36) frame.
#[must_use]
pub fn standby_frame(initiator: u8, destination: u8) -> String {
    format_frame(initiator, destination, CEC_OPCODE_STANDBY, &[])
}

/// Generates `<Active Source>` (opcode 0x82) broadcast frame.
#[must_use]
pub fn active_source_frame(initiator: u8, physical_address: u16) -> String {
    let p_high = (physical_address >> 8) as u8;
    let p_low = (physical_address & 0xFF) as u8;
    format_frame(initiator, 15, CEC_OPCODE_ACTIVE_SOURCE, &[p_high, p_low])
}

/// Generates `<User Control Pressed>` (opcode 0x44) frame.
#[must_use]
pub fn user_control_pressed_frame(initiator: u8, destination: u8, ui_command: u8) -> String {
    format_frame(
        initiator,
        destination,
        CEC_OPCODE_USER_CONTROL_PRESSED,
        &[ui_command],
    )
}

/// Generates `<User Control Released>` (opcode 0x45) frame.
#[must_use]
pub fn user_control_released_frame(initiator: u8, destination: u8) -> String {
    format_frame(
        initiator,
        destination,
        CEC_OPCODE_USER_CONTROL_RELEASED,
        &[],
    )
}

/// Generates `<Give Device Power Status>` (opcode 0x8F) frame.
#[must_use]
pub fn give_power_status_frame(initiator: u8, destination: u8) -> String {
    format_frame(
        initiator,
        destination,
        CEC_OPCODE_GIVE_DEVICE_POWER_STATUS,
        &[],
    )
}

/// Maps a `tv_rs_core::capability::Key` to a CEC UI command code.
pub fn map_key_to_cec(key: Key) -> Result<u8, TvError> {
    match key {
        Key::Select | Key::Enter => Ok(CEC_USER_CONTROL_SELECT),
        Key::Up => Ok(CEC_USER_CONTROL_UP),
        Key::Down => Ok(CEC_USER_CONTROL_DOWN),
        Key::Left => Ok(CEC_USER_CONTROL_LEFT),
        Key::Right => Ok(CEC_USER_CONTROL_RIGHT),
        Key::Home => Ok(CEC_USER_CONTROL_ROOT_MENU),
        Key::Back => Ok(CEC_USER_CONTROL_EXIT),
        Key::Power => Ok(CEC_USER_CONTROL_POWER),
        Key::PowerOff => Ok(CEC_USER_CONTROL_POWER_OFF),
        Key::VolumeUp => Ok(CEC_USER_CONTROL_VOLUME_UP),
        Key::VolumeDown => Ok(CEC_USER_CONTROL_VOLUME_DOWN),
        Key::Mute => Ok(CEC_USER_CONTROL_MUTE),
        Key::Play => Ok(CEC_USER_CONTROL_PLAY),
        Key::Pause => Ok(CEC_USER_CONTROL_PAUSE),
        Key::Stop => Ok(CEC_USER_CONTROL_STOP),
        Key::Rewind => Ok(CEC_USER_CONTROL_REWIND),
        Key::FastForward => Ok(CEC_USER_CONTROL_FAST_FORWARD),
        Key::Digit(d) if d <= 9 => Ok(0x20 + d),
        _ => Err(TvError::Unsupported(tv_rs_core::Capability::Key)),
    }
}

/// Maps an input selector to a 16-bit HDMI physical address.
pub fn map_input_to_physical(input: Input) -> Result<u16, TvError> {
    match input {
        Input::Hdmi(port) if port >= 1 => Ok((port as u16) << 12),
        _ => Err(TvError::Unsupported(tv_rs_core::Capability::InputSelect)),
    }
}

/// Parses `<Report Power Status>` (0x90) from adapter response text.
#[must_use]
pub fn parse_power_status(output: &str) -> PowerState {
    for line in output.lines() {
        let upper = line.to_ascii_uppercase();
        if upper.contains(":90:00") || upper.contains("PWR-STATE: ON") {
            return PowerState::On;
        }
        if upper.contains(":90:01") || upper.contains("PWR-STATE: STANDBY") {
            return PowerState::Standby;
        }
        if upper.contains(":90:02") || upper.contains(":90:03") {
            return PowerState::Standby;
        }
    }
    PowerState::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cec_opcode_frame_formatting() {
        assert_eq!(image_view_on_frame(4, 0), "tx 40:04");
        assert_eq!(standby_frame(4, 0), "tx 40:36");
        assert_eq!(active_source_frame(4, 0x1000), "tx 4F:82:10:00");
        assert_eq!(
            user_control_pressed_frame(4, 0, CEC_USER_CONTROL_SELECT),
            "tx 40:44:00"
        );
        assert_eq!(user_control_released_frame(4, 0), "tx 40:45");
        assert_eq!(give_power_status_frame(4, 0), "tx 40:8F");
    }

    #[test]
    fn test_key_mapping() {
        assert_eq!(
            map_key_to_cec(Key::Select).unwrap(),
            CEC_USER_CONTROL_SELECT
        );
        assert_eq!(map_key_to_cec(Key::Up).unwrap(), CEC_USER_CONTROL_UP);
        assert_eq!(
            map_key_to_cec(Key::VolumeUp).unwrap(),
            CEC_USER_CONTROL_VOLUME_UP
        );
        assert_eq!(map_key_to_cec(Key::Mute).unwrap(), CEC_USER_CONTROL_MUTE);
        assert_eq!(map_key_to_cec(Key::Play).unwrap(), CEC_USER_CONTROL_PLAY);
        assert_eq!(map_key_to_cec(Key::Digit(5)).unwrap(), 0x25);
    }

    #[test]
    fn test_input_to_physical() {
        assert_eq!(map_input_to_physical(Input::Hdmi(1)).unwrap(), 0x1000);
        assert_eq!(map_input_to_physical(Input::Hdmi(2)).unwrap(), 0x2000);
        assert!(map_input_to_physical(Input::Hdmi(0)).is_err());
    }

    #[test]
    fn test_power_status_parsing() {
        assert_eq!(parse_power_status(">> 01:90:00"), PowerState::On);
        assert_eq!(parse_power_status("TRAFFIC: pwr-state: on"), PowerState::On);
        assert_eq!(parse_power_status(">> 01:90:01"), PowerState::Standby);
        assert_eq!(parse_power_status("something else"), PowerState::Unknown);
    }
}
