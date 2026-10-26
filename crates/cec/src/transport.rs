//! The `cec-client` transport: one process per command.
//!
//! `cec-client` is interactive, but its `-s` flag runs a single command and exits,
//! which is exactly the shape this crate needs: no daemon to supervise, no socket to
//! keep alive, and a clear failure when the binary or adapter is absent.

use std::process::Stdio;

use async_trait::async_trait;

use crate::CecError;

/// Sends commands by running `cec-client -s -d 1` once per command.
#[derive(Debug, Clone)]
pub struct ClientTransport {
    binary: String,
    device: u8,
}

impl ClientTransport {
    /// Uses `binary` (looked up on `PATH`) with the first detected adapter.
    #[must_use]
    pub fn new(binary: impl Into<String>) -> Self {
        Self {
            binary: binary.into(),
            device: 1,
        }
    }

    /// Selects a specific adapter index.
    #[must_use]
    pub fn with_device(mut self, device: u8) -> Self {
        self.device = device;
        self
    }

    /// The binary this transport runs.
    #[must_use]
    pub fn binary(&self) -> &str {
        &self.binary
    }
}

#[async_trait]
impl super::CecTransport for ClientTransport {
    async fn send(&self, command: &str) -> Result<String, CecError> {
        use tokio::io::AsyncWriteExt;

        let mut child = tokio::process::Command::new(&self.binary)
            .arg("-s")
            .arg("-d")
            .arg(self.device.to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| match error.kind() {
                std::io::ErrorKind::NotFound => CecError::Unavailable(format!(
                    "'{}' is not on PATH — install cec-utils or point the transport at the binary",
                    self.binary
                )),
                _ => CecError::Io(error.to_string()),
            })?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(format!("{command}\n").as_bytes())
                .await
                .map_err(|error| CecError::Io(error.to_string()))?;
            // Closing stdin is what makes cec-client execute and exit.
            drop(stdin);
        }

        let output = child
            .wait_with_output()
            .await
            .map_err(|error| CecError::Io(error.to_string()))?;

        let mut text = String::from_utf8_lossy(&output.stdout).to_string();
        text.push_str(&String::from_utf8_lossy(&output.stderr));

        if !output.status.success() {
            crate::check(&text)?;
            return Err(CecError::Io(format!(
                "{} exited with {}",
                self.binary, output.status
            )));
        }

        crate::check(&text)?;
        Ok(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CecTransport;

    /// A binary that does not exist must fail as `Unavailable`, not as a generic I/O
    /// error: the agent's healer treats those two differently.
    #[tokio::test]
    async fn a_missing_binary_is_reported_as_unavailable() {
        let transport = ClientTransport::new("definitely-not-a-cec-binary-xyz");
        let error = transport.send("on 0").await.expect_err("missing binary");
        assert!(matches!(error, CecError::Unavailable(_)), "{error:?}");
        assert!(error.to_string().contains("PATH"), "{error}");
    }
}
