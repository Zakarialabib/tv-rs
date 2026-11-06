//! Communication with tv-rs-hostd daemon.
//!
//! Exposes command formatting, session tokens, and RPC endpoints for:
//! - Remote keypresses
//! - Power (wake, sleep, toggle)
//! - Input source selection
//! - Health / state queries

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::control::CompanionInput;
use crate::error::{CompanionError, Result};
use crate::remote::RemoteKey;

/// Payload sent to tv-rs-hostd over HTTP or WebSocket.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum HostdCommand {
    /// Remote key press.
    KeyPress {
        /// Target key.
        key: String,
        /// Ephemeral session token.
        session_token: String,
    },
    /// Set power state.
    SetPower {
        /// Target state: "on" or "standby".
        state: String,
        /// Ephemeral session token.
        session_token: String,
    },
    /// Switch input source.
    SelectInput {
        /// Input source label.
        input: String,
        /// Ephemeral session token.
        session_token: String,
    },
}

/// Client connection for tv-rs-hostd daemon.
#[derive(Debug, Clone)]
pub struct HostdClient {
    host: String,
    port: u16,
    session_token: Option<String>,
    sent_commands: Vec<HostdCommand>,
}

impl HostdClient {
    /// Create new hostd client.
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
            session_token: None,
            sent_commands: Vec::new(),
        }
    }

    /// Set authenticated session token.
    pub fn set_session_token(&mut self, token: impl Into<String>) {
        self.session_token = Some(token.into());
    }

    /// Host IP or hostname.
    pub fn host(&self) -> &str {
        &self.host
    }

    /// Host port.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// History of dispatched commands.
    pub fn sent_commands(&self) -> &[HostdCommand] {
        &self.sent_commands
    }

    fn check_token(&self) -> Result<String> {
        self.session_token
            .clone()
            .ok_or_else(|| CompanionError::Hostd("Unauthenticated: session token missing".into()))
    }

    /// Dispatch remote keypress to hostd.
    pub fn send_key(&mut self, key: RemoteKey) -> Result<()> {
        let token = self.check_token()?;
        let cmd = HostdCommand::KeyPress {
            key: format!("{key:?}"),
            session_token: token,
        };
        self.sent_commands.push(cmd);
        Ok(())
    }

    /// Dispatch power command to hostd.
    pub fn send_power(&mut self, power_on: bool) -> Result<()> {
        let token = self.check_token()?;
        let state = if power_on { "on" } else { "standby" };
        let cmd = HostdCommand::SetPower {
            state: state.into(),
            session_token: token,
        };
        self.sent_commands.push(cmd);
        Ok(())
    }

    /// Dispatch input switch command to hostd.
    pub fn send_input(&mut self, input: CompanionInput) -> Result<()> {
        let token = self.check_token()?;
        let input_str = match input {
            CompanionInput::Hdmi(port) => format!("hdmi{port}"),
            CompanionInput::Other(name) => name,
        };
        let cmd = HostdCommand::SelectInput {
            input: input_str,
            session_token: token,
        };
        self.sent_commands.push(cmd);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hostd_unauthenticated_fails() {
        let mut client = HostdClient::new("192.168.1.50", 8080);
        let err = client
            .send_key(RemoteKey::Home)
            .expect_err("Should fail without token");
        assert!(matches!(err, CompanionError::Hostd(_)));
    }

    #[test]
    fn test_hostd_command_dispatch() {
        let mut client = HostdClient::new("192.168.1.50", 8080);
        client.set_session_token("valid_session_123");

        client.send_key(RemoteKey::Select).unwrap();
        client.send_power(true).unwrap();
        client.send_input(CompanionInput::Hdmi(2)).unwrap();

        assert_eq!(client.sent_commands().len(), 3);
        assert!(matches!(
            client.sent_commands()[0],
            HostdCommand::KeyPress { .. }
        ));
        assert!(matches!(
            client.sent_commands()[1],
            HostdCommand::SetPower { .. }
        ));
        assert!(matches!(
            client.sent_commands()[2],
            HostdCommand::SelectInput { .. }
        ));
    }
}
