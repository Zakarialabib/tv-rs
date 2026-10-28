use crate::error::{LgCmdError, Result};
use crate::protocol::{CMD_KEY, CMD_POWER, CMD_VOLUME, CMD_MUTE};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

/// A client for the LG commercial ASCII serial protocol over TCP.
pub struct LgCmdClient {
    host: String,
    port: u16,
    set_id: u8,
}

impl LgCmdClient {
    /// Creates a new client for a given host and set ID.
    pub fn new(host: impl Into<String>, set_id: u8) -> Self {
        Self {
            host: host.into(),
            port: 9761,
            set_id,
        }
    }

    /// Sets a custom TCP port (default is 9761).
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    /// Sends a command and waits for a response.
    async fn send_command(&self, cmd12: &str, data: &str) -> Result<String> {
        let mut stream = timeout(
            Duration::from_secs(5),
            TcpStream::connect((self.host.as_str(), self.port)),
        )
        .await
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "connection timed out"))??;

        let payload = format!("{} {:02x} {}\r", cmd12, self.set_id, data);
        stream.write_all(payload.as_bytes()).await?;

        let mut buf = [0; 64];
        let mut response = String::new();

        loop {
            let n = timeout(Duration::from_secs(3), stream.read(&mut buf))
                .await
                .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "read timed out"))??;
            if n == 0 {
                break;
            }
            let chunk = String::from_utf8_lossy(&buf[..n]);
            response.push_str(&chunk.as_ref());
            if response.contains('x') {
                break;
            }
        }

        let trimmed = response.trim();
        // Response format: "a 01 OK01x" or "a 01 NG01x"
        if trimmed.contains("NG") {
            return Err(LgCmdError::Rejected(trimmed.to_string()));
        }
        if !trimmed.contains("OK") {
            return Err(LgCmdError::Malformed(trimmed.to_string()));
        }

        Ok(trimmed.to_string())
    }

    /// Turns the TV on or off.
    pub async fn set_power(&self, on: bool) -> Result<()> {
        let data = if on { "01" } else { "00" };
        self.send_command(CMD_POWER, data).await?;
        Ok(())
    }

    /// Sends a remote control key code.
    pub async fn send_key(&self, key_code: u8) -> Result<()> {
        self.send_command(CMD_KEY, &format!("{:02x}", key_code)).await?;
        Ok(())
    }

    /// Sets the volume level (0-100).
    pub async fn set_volume(&self, level: u8) -> Result<()> {
        let level = std::cmp::min(level, 100);
        self.send_command(CMD_VOLUME, &format!("{:02x}", level)).await?;
        Ok(())
    }

    /// Mutes or unmutes the TV.
    pub async fn set_mute(&self, mute: bool) -> Result<()> {
        let data = if mute { "00" } else { "01" };
        self.send_command(CMD_MUTE, data).await?;
        Ok(())
    }
}
