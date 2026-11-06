//! BLE Improv Wi-Fi protocol implementation.
//!
//! Implements the standard Improv BLE GATT service protocol:
//! - Current State (UUID 00467768-6228-2272-4663-277478268001)
//! - Error State (UUID 00467768-6228-2272-4663-277478268002)
//! - RPC Command (UUID 00467768-6228-2272-4663-277478268003)
//! - RPC Result (UUID 00467768-6228-2272-4663-277478268004)
//! - Capabilities (UUID 00467768-6228-2272-4663-277478268005)

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::error::{ProvisionError, Result};

/// Improv Service UUID.
pub const IMPROV_SERVICE_UUID: &str = "00467768-6228-2272-4663-277478268000";
/// Improv Current State Characteristic UUID.
pub const IMPROV_CURRENT_STATE_UUID: &str = "00467768-6228-2272-4663-277478268001";
/// Improv Error State Characteristic UUID.
pub const IMPROV_ERROR_STATE_UUID: &str = "00467768-6228-2272-4663-277478268002";
/// Improv RPC Command Characteristic UUID.
pub const IMPROV_RPC_COMMAND_UUID: &str = "00467768-6228-2272-4663-277478268003";
/// Improv RPC Result Characteristic UUID.
pub const IMPROV_RPC_RESULT_UUID: &str = "00467768-6228-2272-4663-277478268004";
/// Improv Capabilities Characteristic UUID.
pub const IMPROV_CAPABILITIES_UUID: &str = "00467768-6228-2272-4663-277478268005";

/// Improv current state enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[repr(u8)]
pub enum ImprovState {
    /// Device is in pairing/discovery authorization state.
    AuthorizationRequired = 0x01,
    /// Device is ready to receive Wi-Fi credentials.
    Authorized = 0x02,
    /// Device is attempting to connect to the Wi-Fi AP.
    Provisioning = 0x03,
    /// Device successfully connected to Wi-Fi and is provisioned.
    Provisioned = 0x04,
}

impl ImprovState {
    /// Parse from byte representation.
    pub fn from_u8(val: u8) -> Option<Self> {
        match val {
            0x01 => Some(Self::AuthorizationRequired),
            0x02 => Some(Self::Authorized),
            0x03 => Some(Self::Provisioning),
            0x04 => Some(Self::Provisioned),
            _ => None,
        }
    }

    /// As byte.
    pub fn as_u8(&self) -> u8 {
        *self as u8
    }
}

/// Improv error state enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[repr(u8)]
pub enum ImprovErrorState {
    /// No error present.
    NoError = 0x00,
    /// Invalid command or packet framing received.
    InvalidRpcPacket = 0x01,
    /// Command sent when device was not in correct state.
    UnknownCommand = 0x02,
    /// Connect failed due to bad credentials, timeout or AP refusal.
    UnableToConnect = 0x03,
    /// Device not authorized to perform operation.
    NotAuthorized = 0x04,
    /// Unknown error occurred.
    UnknownError = 0xFF,
}

impl ImprovErrorState {
    /// Parse from byte representation.
    pub fn from_u8(val: u8) -> Self {
        match val {
            0x00 => Self::NoError,
            0x01 => Self::InvalidRpcPacket,
            0x02 => Self::UnknownCommand,
            0x03 => Self::UnableToConnect,
            0x04 => Self::NotAuthorized,
            _ => Self::UnknownError,
        }
    }

    /// As byte.
    pub fn as_u8(&self) -> u8 {
        *self as u8
    }
}

/// Improv RPC commands.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ImprovCommand {
    /// Send Wi-Fi credentials: SSID and Password.
    SendWifiCredentials {
        /// SSID of target Wi-Fi network.
        ssid: String,
        /// Password of target Wi-Fi network.
        password: String,
    },
    /// Request the device to identify itself (e.g. blink LED, show on screen).
    Identify,
}

impl ImprovCommand {
    /// Command type byte for Improv protocol.
    pub fn command_type(&self) -> u8 {
        match self {
            Self::SendWifiCredentials { .. } => 0x01,
            Self::Identify => 0x03,
        }
    }

    /// Encode into Improv packet byte vector:
    /// `[command_id, data_length, (string_len, string_bytes)*, checksum]`
    pub fn encode(&self) -> Vec<u8> {
        let mut data = Vec::new();
        match self {
            Self::SendWifiCredentials { ssid, password } => {
                let s_bytes = ssid.as_bytes();
                data.push(s_bytes.len() as u8);
                data.extend_from_slice(s_bytes);

                let p_bytes = password.as_bytes();
                data.push(p_bytes.len() as u8);
                data.extend_from_slice(p_bytes);
            }
            Self::Identify => {}
        }

        let mut packet = Vec::with_capacity(data.len() + 3);
        packet.push(self.command_type());
        packet.push(data.len() as u8);
        packet.extend_from_slice(&data);

        let checksum = packet.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
        packet.push(checksum);
        packet
    }

    /// Decode an Improv command packet from raw bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 3 {
            return Err(ProvisionError::ImprovFrame(
                "Packet too short (minimum 3 bytes)".into(),
            ));
        }

        let cmd_type = bytes[0];
        let data_len = bytes[1] as usize;
        let expected_total_len = 2 + data_len + 1;

        if bytes.len() != expected_total_len {
            return Err(ProvisionError::ImprovFrame(format!(
                "Length mismatch: header specifies {data_len} data bytes, packet has {} total",
                bytes.len()
            )));
        }

        let payload = &bytes[2..2 + data_len];
        let received_checksum = bytes[bytes.len() - 1];
        let calculated_checksum = bytes[..bytes.len() - 1]
            .iter()
            .fold(0u8, |acc, &b| acc.wrapping_add(b));

        if received_checksum != calculated_checksum {
            return Err(ProvisionError::ImprovFrame(format!(
                "Checksum error: expected {calculated_checksum}, got {received_checksum}"
            )));
        }

        match cmd_type {
            0x01 => {
                // Parse strings: [len1, str1..., len2, str2...]
                if payload.is_empty() {
                    return Err(ProvisionError::ImprovFrame(
                        "Empty payload for SendWifiCredentials".into(),
                    ));
                }
                let ssid_len = payload[0] as usize;
                if payload.len() < 1 + ssid_len + 1 {
                    return Err(ProvisionError::ImprovFrame(
                        "Malformed SSID length in payload".into(),
                    ));
                }
                let ssid = core::str::from_utf8(&payload[1..1 + ssid_len])
                    .map_err(|e| ProvisionError::ImprovFrame(format!("Invalid UTF-8 SSID: {e}")))?
                    .into();

                let rem = &payload[1 + ssid_len..];
                let pass_len = rem[0] as usize;
                if rem.len() < 1 + pass_len {
                    return Err(ProvisionError::ImprovFrame(
                        "Malformed Password length in payload".into(),
                    ));
                }
                let password = core::str::from_utf8(&rem[1..1 + pass_len])
                    .map_err(|e| {
                        ProvisionError::ImprovFrame(format!("Invalid UTF-8 Password: {e}"))
                    })?
                    .into();

                Ok(Self::SendWifiCredentials { ssid, password })
            }
            0x03 => Ok(Self::Identify),
            other => Err(ProvisionError::ImprovFrame(format!(
                "Unknown command type 0x{other:02X}"
            ))),
        }
    }
}

/// Improv RPC Result frame.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ImprovRpcResult {
    /// Command type this result corresponds to.
    pub command_type: u8,
    /// Result values/URLs returned from device (e.g. redirect URL).
    pub values: Vec<String>,
}

impl ImprovRpcResult {
    /// Encode RPC result into Improv GATT packet.
    pub fn encode(&self) -> Vec<u8> {
        let mut data = Vec::new();
        for val in &self.values {
            let b = val.as_bytes();
            data.push(b.len() as u8);
            data.extend_from_slice(b);
        }

        let mut packet = Vec::with_capacity(data.len() + 3);
        packet.push(self.command_type);
        packet.push(data.len() as u8);
        packet.extend_from_slice(&data);

        let checksum = packet.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
        packet.push(checksum);
        packet
    }

    /// Decode RPC result from raw bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 3 {
            return Err(ProvisionError::ImprovFrame("Packet too short".into()));
        }

        let command_type = bytes[0];
        let data_len = bytes[1] as usize;
        if bytes.len() != 2 + data_len + 1 {
            return Err(ProvisionError::ImprovFrame("Packet length mismatch".into()));
        }

        let received_checksum = bytes[bytes.len() - 1];
        let calculated_checksum = bytes[..bytes.len() - 1]
            .iter()
            .fold(0u8, |acc, &b| acc.wrapping_add(b));

        if received_checksum != calculated_checksum {
            return Err(ProvisionError::ImprovFrame("Invalid checksum".into()));
        }

        let mut values = Vec::new();
        let mut idx = 2;
        let end = 2 + data_len;

        while idx < end {
            let len = bytes[idx] as usize;
            idx += 1;
            if idx + len > end {
                return Err(ProvisionError::ImprovFrame(
                    "String exceeds payload bounds".into(),
                ));
            }
            let s = core::str::from_utf8(&bytes[idx..idx + len])
                .map_err(|e| ProvisionError::ImprovFrame(format!("UTF-8 error: {e}")))?;
            values.push(s.into());
            idx += len;
        }

        Ok(Self {
            command_type,
            values,
        })
    }
}

/// Simulated or hardware BLE Improv state machine for display provisioning.
#[derive(Debug, Clone)]
pub struct ImprovSession {
    state: ImprovState,
    error: ImprovErrorState,
    provisioned_ssid: Option<String>,
}

impl Default for ImprovSession {
    fn default() -> Self {
        Self::new()
    }
}

impl ImprovSession {
    /// Initialize a new Improv session.
    pub fn new() -> Self {
        Self {
            state: ImprovState::Authorized,
            error: ImprovErrorState::NoError,
            provisioned_ssid: None,
        }
    }

    /// Current state.
    pub fn state(&self) -> ImprovState {
        self.state
    }

    /// Current error.
    pub fn error(&self) -> ImprovErrorState {
        self.error
    }

    /// Provisioned SSID if any.
    pub fn provisioned_ssid(&self) -> Option<&str> {
        self.provisioned_ssid.as_deref()
    }

    /// Authorize session.
    pub fn authorize(&mut self) {
        self.state = ImprovState::Authorized;
        self.error = ImprovErrorState::NoError;
    }

    /// Execute incoming RPC command against session.
    pub fn handle_command(&mut self, cmd: ImprovCommand) -> Result<ImprovRpcResult> {
        match cmd {
            ImprovCommand::Identify => {
                tracing::info!("Improv session Identify requested");
                Ok(ImprovRpcResult {
                    command_type: 0x03,
                    values: Vec::new(),
                })
            }
            ImprovCommand::SendWifiCredentials { ssid, password } => {
                if self.state == ImprovState::AuthorizationRequired {
                    self.error = ImprovErrorState::NotAuthorized;
                    return Err(ProvisionError::ImprovState(
                        "Cannot provision: authorization required".into(),
                    ));
                }

                if ssid.trim().is_empty() {
                    self.error = ImprovErrorState::UnableToConnect;
                    return Err(ProvisionError::ImprovFrame("Empty SSID".into()));
                }

                // Simulate connecting
                self.state = ImprovState::Provisioning;

                // Validate password format
                if password.len() < 8 && !password.is_empty() {
                    self.error = ImprovErrorState::UnableToConnect;
                    return Err(ProvisionError::ImprovState(
                        "Wi-Fi connection failed: password too short".into(),
                    ));
                }

                self.provisioned_ssid = Some(ssid.clone());
                self.state = ImprovState::Provisioned;
                self.error = ImprovErrorState::NoError;

                Ok(ImprovRpcResult {
                    command_type: 0x01,
                    values: alloc::vec![format!("http://192.168.1.50:8080/setup?ssid={ssid}")],
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_improv_command_encoding_and_decoding() {
        let cmd = ImprovCommand::SendWifiCredentials {
            ssid: "Corporate-Display".into(),
            password: "supersecretpass".into(),
        };

        let encoded = cmd.encode();
        let decoded = ImprovCommand::decode(&encoded).expect("Decode should succeed");
        assert_eq!(decoded, cmd);
    }

    #[test]
    fn test_improv_identify_command() {
        let cmd = ImprovCommand::Identify;
        let encoded = cmd.encode();
        let decoded = ImprovCommand::decode(&encoded).expect("Decode identify");
        assert_eq!(decoded, cmd);
    }

    #[test]
    fn test_improv_checksum_validation() {
        let cmd = ImprovCommand::Identify;
        let mut encoded = cmd.encode();
        // Tamper with checksum
        let len = encoded.len();
        encoded[len - 1] = encoded[len - 1].wrapping_add(1);

        let err = ImprovCommand::decode(&encoded).expect_err("Should detect checksum error");
        assert!(err.to_string().contains("Checksum error"));
    }

    #[test]
    fn test_improv_session_flow() {
        let mut session = ImprovSession::new();
        assert_eq!(session.state(), ImprovState::Authorized);

        let res = session
            .handle_command(ImprovCommand::SendWifiCredentials {
                ssid: "TestWiFi".into(),
                password: "validpassword123".into(),
            })
            .expect("provisioning successful");

        assert_eq!(session.state(), ImprovState::Provisioned);
        assert_eq!(session.provisioned_ssid(), Some("TestWiFi"));
        assert_eq!(res.command_type, 0x01);
        assert!(!res.values.is_empty());
    }
}
