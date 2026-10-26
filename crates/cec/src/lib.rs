//! HDMI-CEC: the one path that still works when the network does not.
//!
//! Every other crate here assumes the television answers on the LAN. When it does
//! not — a panel on a different VLAN, a set with Wi-Fi disabled, a display that never
//! had a network stack at all — the only control left is the HDMI cable. CEC can power
//! a panel on and put it in standby over that cable, and nothing else in this workspace
//! can.
//!
//! # Why this crate is optional and separate
//!
//! CEC needs hardware: a Pulse-Eight-class USB adapter, or a Raspberry Pi with CEC on
//! its HDMI port. Most machines have neither, so this crate is feature-gated, is not a
//! dependency of the agent by default, and compiles to a thin library when the
//! `client` feature is off.
//!
//! It also does not link `libcec`. It shells out to an existing `cec-client` binary
//! instead: no `unsafe`, no native build step, and the failure mode when the tool is
//! missing is a clear error rather than a link failure.
//!
//! ```no_run
//! use tv_rs_cec::{CecAddress, CecClient, CecError, CecTransport};
//!
//! // With `--features client`, `T` is `ClientTransport::new("cec-client")`;
//! // anything implementing `CecTransport` works, which is how the tests run it
//! // without an adapter.
//! async fn power_on<T: CecTransport>(cec: &CecClient<T>) -> Result<String, CecError> {
//!     cec.power_on(CecAddress::Tv).await
//! }
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::fmt;

use async_trait::async_trait;

#[cfg(feature = "client")]
pub mod transport;

#[cfg(feature = "client")]
pub use transport::{ClientTransport, PersistentClientTransport};

/// A logical address on the CEC bus.
///
/// Only the addresses a signage install actually needs are modelled; the rest are
/// vendor-specific and have no business in an API surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CecAddress {
    /// The television. Always address 0.
    Tv,
    /// Playback device 1 — the usual address for the first source.
    Playback1,
    /// Playback device 2.
    Playback2,
    /// Audio system.
    AudioSystem,
    /// Everyone on the bus.
    Broadcast,
}

impl CecAddress {
    /// The numeric logical address.
    #[must_use]
    pub fn as_u8(&self) -> u8 {
        match self {
            Self::Tv => 0,
            Self::Playback1 => 4,
            Self::Playback2 => 8,
            Self::AudioSystem => 5,
            Self::Broadcast => 15,
        }
    }
}

impl fmt::Display for CecAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_u8())
    }
}

impl TryFrom<u8> for CecAddress {
    type Error = CecError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Tv),
            4 => Ok(Self::Playback1),
            8 => Ok(Self::Playback2),
            5 => Ok(Self::AudioSystem),
            15 => Ok(Self::Broadcast),
            _ => Err(CecError::Rejected(format!(
                "unsupported or invalid cec logical address: {value}"
            ))),
        }
    }
}

impl TryFrom<&str> for CecAddress {
    type Error = CecError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let trimmed = value.trim();
        if let Ok(num) = trimmed.parse::<u8>() {
            return Self::try_from(num);
        }
        if trimmed.starts_with("0x") || trimmed.starts_with("0X") {
            if let Ok(num) = u8::from_str_radix(&trimmed[2..], 16) {
                return Self::try_from(num);
            }
        }
        match trimmed.to_ascii_lowercase().as_str() {
            "tv" => Ok(Self::Tv),
            "playback1" | "playback_1" => Ok(Self::Playback1),
            "playback2" | "playback_2" => Ok(Self::Playback2),
            "audiosystem" | "audio_system" | "audio" => Ok(Self::AudioSystem),
            "broadcast" => Ok(Self::Broadcast),
            _ => Err(CecError::Rejected(format!(
                "invalid cec address string: '{value}'"
            ))),
        }
    }
}

/// A structured CEC command line that can be executed by a CEC adapter or `cec-client`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CecCommand {
    /// Powers the addressed device on (e.g. `on 0`).
    PowerOn(CecAddress),
    /// Puts the addressed device into standby (e.g. `standby 0`).
    Standby(CecAddress),
    /// Broadcasts active source for physical HDMI address (e.g. `tx 0F:82:10:00`).
    SetActiveSource(u16),
    /// A custom hex frame (e.g. `tx 0F:82:10:00` or `10:00`).
    CustomHexFrame(String),
    /// A raw command string for `cec-client`.
    Raw(String),
}

impl CecCommand {
    /// Validates the command string representation for security and grammar compliance.
    ///
    /// # Errors
    ///
    /// Returns [`CecError::Rejected`] if the command is empty or contains control
    /// characters or newlines.
    pub fn validate(&self) -> Result<(), CecError> {
        let cmd_str = self.to_string();
        let trimmed = cmd_str.trim();
        if trimmed.is_empty() || !cmd_str.chars().all(|c| c.is_ascii_graphic() || c == ' ') {
            return Err(CecError::Rejected(format!(
                "invalid cec command: '{cmd_str}' contains illegal control characters, newlines, or is empty"
            )));
        }
        Ok(())
    }
}

impl fmt::Display for CecCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PowerOn(addr) => write!(f, "on {addr}"),
            Self::Standby(addr) => write!(f, "standby {addr}"),
            Self::SetActiveSource(phys) => {
                let high = (phys >> 8) as u8;
                let low = (phys & 0x00ff) as u8;
                write!(f, "tx 0F:82:{high:02X}:{low:02X}")
            }
            Self::CustomHexFrame(frame) => {
                let trimmed = frame.trim();
                if trimmed.to_ascii_lowercase().starts_with("tx ") {
                    write!(f, "{trimmed}")
                } else {
                    write!(f, "tx {trimmed}")
                }
            }
            Self::Raw(raw) => write!(f, "{}", raw.trim()),
        }
    }
}

fn parse_physical_address(s: &str) -> Result<u16, CecError> {
    let clean = s.replace([':', '.'], "");
    let hex_str = clean.strip_prefix("0x").or_else(|| clean.strip_prefix("0X")).unwrap_or(&clean);
    u16::from_str_radix(hex_str, 16).map_err(|_| {
        CecError::Rejected(format!("invalid physical address format: '{s}'"))
    })
}

fn parse_active_source_frame(hex_part: &str) -> Option<u16> {
    let clean = hex_part.replace([':', ' '], "");
    if clean.len() == 8 && clean.get(0..4).map(|s| s.to_ascii_uppercase()) == Some("0F82".to_string()) {
        u16::from_str_radix(&clean[4..8], 16).ok()
    } else {
        None
    }
}

fn is_valid_hex_frame(s: &str) -> bool {
    let clean = s.trim();
    if clean.is_empty() {
        return false;
    }
    let hex_part = clean.strip_prefix("tx ").unwrap_or(clean);
    let stripped = hex_part.replace([':', ' '], "");
    !stripped.is_empty() && stripped.len().is_multiple_of(2) && stripped.chars().all(|c| c.is_ascii_hexdigit())
}

impl TryFrom<&str> for CecCommand {
    type Error = CecError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let trimmed = value.trim();
        if trimmed.is_empty() || !value.chars().all(|c| c.is_ascii_graphic() || c == ' ') {
            return Err(CecError::Rejected(format!(
                "invalid cec command: '{value}' contains illegal control characters, newlines, or is empty"
            )));
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.is_empty() {
            return Err(CecError::Rejected("empty cec command".to_string()));
        }

        match parts[0].to_ascii_lowercase().as_str() {
            "on" => {
                if parts.len() == 2 {
                    let addr = CecAddress::try_from(parts[1])?;
                    Ok(CecCommand::PowerOn(addr))
                } else {
                    Err(CecError::Rejected(format!("invalid 'on' command format: '{trimmed}'")))
                }
            }
            "standby" | "st" => {
                if parts.len() == 2 {
                    let addr = CecAddress::try_from(parts[1])?;
                    Ok(CecCommand::Standby(addr))
                } else {
                    Err(CecError::Rejected(format!("invalid 'standby' command format: '{trimmed}'")))
                }
            }
            "as" | "active_source" => {
                if parts.len() == 1 {
                    Ok(CecCommand::SetActiveSource(0x1000))
                } else if parts.len() == 2 {
                    let phys = parse_physical_address(parts[1])?;
                    Ok(CecCommand::SetActiveSource(phys))
                } else {
                    Err(CecError::Rejected(format!("invalid active source command format: '{trimmed}'")))
                }
            }
            "tx" => {
                if parts.len() > 1 {
                    let hex_part = parts[1..].join(" ");
                    if let Some(phys) = parse_active_source_frame(&hex_part) {
                        Ok(CecCommand::SetActiveSource(phys))
                    } else if is_valid_hex_frame(&hex_part) {
                        Ok(CecCommand::CustomHexFrame(trimmed.to_string()))
                    } else {
                        Err(CecError::Rejected(format!("invalid hex frame payload in 'tx': '{trimmed}'")))
                    }
                } else {
                    Err(CecError::Rejected("missing payload after 'tx'".to_string()))
                }
            }
            _ => {
                if is_valid_hex_frame(trimmed) {
                    if let Some(phys) = parse_active_source_frame(trimmed) {
                        Ok(CecCommand::SetActiveSource(phys))
                    } else {
                        Ok(CecCommand::CustomHexFrame(trimmed.to_string()))
                    }
                } else {
                    Ok(CecCommand::Raw(trimmed.to_string()))
                }
            }
        }
    }
}

impl TryFrom<String> for CecCommand {
    type Error = CecError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        CecCommand::try_from(value.as_str())
    }
}

/// Why a CEC command could not be sent.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CecError {
    /// No adapter or client binary is present.
    #[error("cec unavailable: {0}")]
    Unavailable(String),
    /// The adapter answered, and refused the command.
    #[error("cec rejected the command: {0}")]
    Rejected(String),
    /// The process or pipe failed.
    #[error("cec transport failed: {0}")]
    Io(String),
}

/// Sends one command line to a CEC adapter.
///
/// The trait exists so the client can be tested without hardware, and so an
/// installer can wire in `libcec` FFI, a TCP bridge or a mocked bus if they want to.
#[async_trait]
pub trait CecTransport: Send + Sync {
    /// Sends `command` (the `cec-client` command language) and returns its output.
    async fn send(&self, command: &str) -> Result<String, CecError>;

    /// Sends a structured [`CecCommand`] and returns its output.
    async fn send_command(&self, command: &CecCommand) -> Result<String, CecError> {
        command.validate()?;
        self.send(&command.to_string()).await
    }
}

/// CEC operations, over any transport.
pub struct CecClient<T: CecTransport> {
    transport: T,
}

impl<T: CecTransport> CecClient<T> {
    /// Wraps a transport.
    pub fn new(transport: T) -> Self {
        Self { transport }
    }

    /// The underlying transport.
    pub fn transport(&self) -> &T {
        &self.transport
    }

    /// Sends a structured [`CecCommand`].
    pub async fn send_command(&self, command: CecCommand) -> Result<String, CecError> {
        command.validate()?;
        let output = self.transport.send_command(&command).await?;
        check(&output)?;
        Ok(output)
    }

    /// Sends a raw or text command string, parsing it into a [`CecCommand`].
    pub async fn send(&self, command: &str) -> Result<String, CecError> {
        let cmd = CecCommand::try_from(command)?;
        self.send_command(cmd).await
    }

    /// Powers the addressed device on.
    pub async fn power_on(&self, address: CecAddress) -> Result<String, CecError> {
        self.send_command(CecCommand::PowerOn(address)).await
    }

    /// Puts the addressed device into standby.
    pub async fn standby(&self, address: CecAddress) -> Result<String, CecError> {
        self.send_command(CecCommand::Standby(address)).await
    }

    /// Makes this device the active source with the given physical address.
    ///
    /// `physical` is the HDMI port topology as hexadecimal digits, e.g. `0x1000`
    /// for "input 1, port 0". Sending this is what makes a panel switch to the
    /// player's input without a remote.
    pub async fn set_active_source(&self, physical: u16) -> Result<String, CecError> {
        self.send_command(CecCommand::SetActiveSource(physical)).await
    }
}

/// Reads a `cec-client` reply and turns a reported failure into an error.
///
/// `cec-client` exits zero even when the adapter refuses a command, so the exit code
/// is not the signal; the text is. The check is deliberately narrow — a false
/// positive here would turn a working command into a failure report.
///
/// # Errors
///
/// Returns [`CecError::Rejected`] when the output says the command was refused and
/// [`CecError::Unavailable`] when no adapter was found.
pub fn check(output: &str) -> Result<(), CecError> {
    for line in output.lines() {
        let upper = line.to_ascii_uppercase();
        if upper.contains("NO CEC ADAPTER") || upper.contains("NO ADAPTER FOUND") {
            return Err(CecError::Unavailable(line.trim().to_string()));
        }
        if upper.contains("ERROR:") || upper.contains("NOT SUCCESSFULL") {
            return Err(CecError::Rejected(line.trim().to_string()));
        }
    }
    Ok(())
}

/// Formats the frame for an active-source broadcast, exposed for tests and logging.
#[must_use]
pub fn active_source_frame(physical: u16) -> String {
    let high = (physical >> 8) as u8;
    let low = (physical & 0x00ff) as u8;
    format!("tx 0F:82:{high:02X}:{low:02X}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Recorder {
        sent: Mutex<Vec<String>>,
        reply: Option<&'static str>,
    }

    #[async_trait]
    impl CecTransport for Recorder {
        async fn send(&self, command: &str) -> Result<String, CecError> {
            self.sent.lock().unwrap().push(command.to_string());
            Ok(self.reply.unwrap_or("").to_string())
        }
    }

    fn recorder(reply: Option<&'static str>) -> std::sync::Arc<Recorder> {
        std::sync::Arc::new(Recorder {
            sent: Mutex::default(),
            reply,
        })
    }

    #[tokio::test]
    async fn power_and_standby_use_the_documented_command_language() {
        let recorder = recorder(None);
        let client = CecClient::new(Plumbing(recorder.clone()));
        client.power_on(CecAddress::Tv).await.unwrap();
        client.standby(CecAddress::Playback1).await.unwrap();

        let sent = recorder.sent.lock().unwrap().clone();
        assert_eq!(sent, vec!["on 0".to_string(), "standby 4".to_string()]);
    }

    #[tokio::test]
    async fn an_active_source_frame_encodes_the_physical_address() {
        let recorder = recorder(None);
        let client = CecClient::new(Plumbing(recorder.clone()));
        client.set_active_source(0x1000).await.unwrap();
        assert_eq!(recorder.sent.lock().unwrap()[0], "tx 0F:82:10:00");
        assert_eq!(active_source_frame(0x2300), "tx 0F:82:23:00");
    }

    #[test]
    fn a_missing_adapter_is_unavailable_not_rejected() {
        let error = check("CEC bus information\nNo CEC adapter found").expect_err("missing");
        assert!(matches!(error, CecError::Unavailable(_)), "{error:?}");
    }

    #[test]
    fn a_refused_command_is_rejected() {
        let error = check("TRAFFIC: ERROR: command not supported").expect_err("rejected");
        assert!(matches!(error, CecError::Rejected(_)), "{error:?}");
    }

    #[test]
    fn ordinary_traffic_is_not_an_error() {
        // A false positive here would report a working panel as broken.
        assert!(check("TRAFFIC: [ 10 ] -> 0F\n").is_ok());
        assert!(check("").is_ok());
    }

    #[test]
    fn cec_address_conversions() {
        assert_eq!(CecAddress::try_from(0).unwrap(), CecAddress::Tv);
        assert_eq!(CecAddress::try_from(4).unwrap(), CecAddress::Playback1);
        assert_eq!(CecAddress::try_from(8).unwrap(), CecAddress::Playback2);
        assert_eq!(CecAddress::try_from(5).unwrap(), CecAddress::AudioSystem);
        assert_eq!(CecAddress::try_from(15).unwrap(), CecAddress::Broadcast);
        assert!(CecAddress::try_from(99).is_err());

        assert_eq!(CecAddress::try_from("0").unwrap(), CecAddress::Tv);
        assert_eq!(CecAddress::try_from("tv").unwrap(), CecAddress::Tv);
        assert_eq!(CecAddress::try_from("playback1").unwrap(), CecAddress::Playback1);
        assert_eq!(CecAddress::try_from("playback_2").unwrap(), CecAddress::Playback2);
        assert_eq!(CecAddress::try_from("audio").unwrap(), CecAddress::AudioSystem);
        assert_eq!(CecAddress::try_from("broadcast").unwrap(), CecAddress::Broadcast);
        assert!(CecAddress::try_from("invalid_addr").is_err());
    }

    #[test]
    fn cec_command_display_and_parsing() {
        let cmd_on = CecCommand::try_from("on tv").unwrap();
        assert_eq!(cmd_on, CecCommand::PowerOn(CecAddress::Tv));
        assert_eq!(cmd_on.to_string(), "on 0");

        let cmd_st = CecCommand::try_from("standby 4").unwrap();
        assert_eq!(cmd_st, CecCommand::Standby(CecAddress::Playback1));
        assert_eq!(cmd_st.to_string(), "standby 4");

        let cmd_as = CecCommand::try_from("as 0x1000").unwrap();
        assert_eq!(cmd_as, CecCommand::SetActiveSource(0x1000));
        assert_eq!(cmd_as.to_string(), "tx 0F:82:10:00");

        let cmd_as_frame = CecCommand::try_from("tx 0F:82:20:00").unwrap();
        assert_eq!(cmd_as_frame, CecCommand::SetActiveSource(0x2000));
        assert_eq!(cmd_as_frame.to_string(), "tx 0F:82:20:00");

        let cmd_custom = CecCommand::try_from("tx 10:44:41").unwrap();
        assert_eq!(cmd_custom, CecCommand::CustomHexFrame("tx 10:44:41".to_string()));
        assert_eq!(cmd_custom.to_string(), "tx 10:44:41");

        let cmd_raw = CecCommand::try_from("volup").unwrap();
        assert_eq!(cmd_raw, CecCommand::Raw("volup".to_string()));
        assert_eq!(cmd_raw.to_string(), "volup");
    }

    #[test]
    fn cec_command_grammar_validation() {
        assert!(CecCommand::try_from("on tv\nstandby tv").is_err());
        assert!(CecCommand::try_from("on tv\0").is_err());
        assert!(CecCommand::try_from("").is_err());
        assert!(CecCommand::try_from("   ").is_err());

        let invalid_raw = CecCommand::Raw("bad\ncommand".to_string());
        assert!(invalid_raw.validate().is_err());
    }

    #[tokio::test]
    async fn cec_client_send_command_and_string() {
        let recorder = recorder(None);
        let client = CecClient::new(Plumbing(recorder.clone()));

        client.send_command(CecCommand::PowerOn(CecAddress::Tv)).await.unwrap();
        client.send("standby 0").await.unwrap();

        let sent = recorder.sent.lock().unwrap().clone();
        assert_eq!(sent, vec!["on 0".to_string(), "standby 0".to_string()]);
    }

    /// Adapts an `Arc<Recorder>` to the transport trait without cloning the log.
    struct Plumbing(std::sync::Arc<Recorder>);

    #[async_trait]
    impl CecTransport for Plumbing {
        async fn send(&self, command: &str) -> Result<String, CecError> {
            self.0.send(command).await
        }
    }
}
