//! CEC operations, over any transport.

use crate::cec::error::CecError;
use crate::cec::transport::CecTransport;
use crate::cec::types::{CecAddress, CecCommand};

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
    pub async fn set_active_source(&self, physical: u16) -> Result<String, CecError> {
        self.send_command(CecCommand::SetActiveSource(physical))
            .await
    }
}

/// Reads a `cec-client` reply and turns a reported failure into an error.
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
    use async_trait::async_trait;
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

    struct Plumbing(std::sync::Arc<Recorder>);

    #[async_trait]
    impl CecTransport for Plumbing {
        async fn send(&self, command: &str) -> Result<String, CecError> {
            self.0.send(command).await
        }
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
        assert!(check("TRAFFIC: [ 10 ] -> 0F\n").is_ok());
        assert!(check("").is_ok());
    }
}
