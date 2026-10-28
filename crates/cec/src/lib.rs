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
pub use transport::ClientTransport;

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

    /// Powers the addressed device on.
    pub async fn power_on(&self, address: CecAddress) -> Result<String, CecError> {
        let output = self.transport.send(&format!("on {address}")).await?;
        check(&output)?;
        Ok(output)
    }

    /// Puts the addressed device into standby.
    pub async fn standby(&self, address: CecAddress) -> Result<String, CecError> {
        let output = self.transport.send(&format!("standby {address}")).await?;
        check(&output)?;
        Ok(output)
    }

    /// Makes this device the active source with the given physical address.
    ///
    /// `physical` is the HDMI port topology as hexadecimal digits, e.g. `0x1000`
    /// for "input 1, port 0". Sending this is what makes a panel switch to the
    /// player's input without a remote.
    pub async fn set_active_source(&self, physical: u16) -> Result<String, CecError> {
        let high = (physical >> 8) as u8;
        let low = (physical & 0x00ff) as u8;
        let frame = format!("tx 0F:82:{high:02X}:{low:02X}");
        let output = self.transport.send(&frame).await?;
        check(&output)?;
        Ok(output)
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

    /// Adapts an `Arc<Recorder>` to the transport trait without cloning the log.
    struct Plumbing(std::sync::Arc<Recorder>);

    #[async_trait]
    impl CecTransport for Plumbing {
        async fn send(&self, command: &str) -> Result<String, CecError> {
            self.0.send(command).await
        }
    }
}
