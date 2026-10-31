//! The `cec-client` transport: one process per command.
//!
//! `cec-client` is interactive, but its `-s` flag runs a single command and exits,
//! which is exactly the shape this crate needs: no daemon to supervise, no socket to
//! keep alive, and a clear failure when the binary or adapter is absent.

use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::Mutex;

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

        let cmd = crate::CecCommand::try_from(command)?;

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
                .write_all(format!("{cmd}\n").as_bytes())
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

struct PersistentProcess {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl PersistentProcess {
    fn spawn(binary: &str, device: u8) -> Result<Self, CecError> {
        let mut child = tokio::process::Command::new(binary)
            .arg("-d")
            .arg(device.to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| match error.kind() {
                std::io::ErrorKind::NotFound => CecError::Unavailable(format!(
                    "'{binary}' is not on PATH — install cec-utils or point the transport at the binary"
                )),
                _ => CecError::Io(error.to_string()),
            })?;

        let stdin = child.stdin.take().ok_or_else(|| {
            CecError::Io("failed to open stdin for cec-client process".to_string())
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            CecError::Io("failed to open stdout for cec-client process".to_string())
        })?;

        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
        })
    }
}

/// Sends commands over a long-lived `cec-client` process session.
///
/// Spawns `cec-client` once and reuses stdin/stdout process pipes for multiple commands,
/// eliminating process spawn overhead across commands.
pub struct PersistentClientTransport {
    binary: String,
    device: u8,
    read_timeout: Duration,
    process: Mutex<Option<PersistentProcess>>,
}

impl std::fmt::Debug for PersistentClientTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PersistentClientTransport")
            .field("binary", &self.binary)
            .field("device", &self.device)
            .field("read_timeout", &self.read_timeout)
            .finish()
    }
}

impl PersistentClientTransport {
    /// Uses `binary` (looked up on `PATH`) with the first detected adapter.
    #[must_use]
    pub fn new(binary: impl Into<String>) -> Self {
        Self {
            binary: binary.into(),
            device: 1,
            read_timeout: Duration::from_millis(500),
            process: Mutex::new(None),
        }
    }

    /// Selects a specific adapter index.
    #[must_use]
    pub fn with_device(mut self, device: u8) -> Self {
        self.device = device;
        self
    }

    /// Sets the timeout for reading command response lines from stdout.
    #[must_use]
    pub fn with_read_timeout(mut self, timeout: Duration) -> Self {
        self.read_timeout = timeout;
        self
    }

    /// The binary this transport runs.
    #[must_use]
    pub fn binary(&self) -> &str {
        &self.binary
    }
}

#[async_trait]
impl super::CecTransport for PersistentClientTransport {
    async fn send(&self, command: &str) -> Result<String, CecError> {
        let cmd = crate::CecCommand::try_from(command)?;

        let mut lock = self.process.lock().await;

        if let Some(proc) = lock.as_mut() {
            if let Ok(Some(_)) = proc.child.try_wait() {
                *lock = None;
            }
        }

        if lock.is_none() {
            *lock = Some(PersistentProcess::spawn(&self.binary, self.device)?);
        }

        let proc = lock.as_mut().unwrap();

        if let Err(err) = proc.stdin.write_all(format!("{cmd}\n").as_bytes()).await {
            *lock = None;
            return Err(CecError::Io(format!(
                "failed to write command to cec-client stdin: {err}"
            )));
        }
        if let Err(err) = proc.stdin.flush().await {
            *lock = None;
            return Err(CecError::Io(format!(
                "failed to flush cec-client stdin: {err}"
            )));
        }

        let mut output = String::new();
        let mut line = String::new();

        loop {
            line.clear();
            match tokio::time::timeout(self.read_timeout, proc.stdout.read_line(&mut line)).await {
                Ok(Ok(0)) => {
                    *lock = None;
                    break;
                }
                Ok(Ok(_)) => {
                    output.push_str(&line);
                }
                Ok(Err(err)) => {
                    *lock = None;
                    return Err(CecError::Io(format!(
                        "failed to read from cec-client stdout: {err}"
                    )));
                }
                Err(_) => {
                    break;
                }
            }
        }

        crate::check(&output)?;
        Ok(output)
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

    #[tokio::test]
    async fn injected_newlines_are_rejected() {
        let transport = ClientTransport::new("definitely-not-a-cec-binary-xyz");
        let error = transport
            .send("on 0\nstandby 0")
            .await
            .expect_err("newline injected");
        assert!(matches!(error, CecError::Rejected(_)), "{error:?}");

        let error_crlf = transport
            .send("on 0\r\nstandby 0")
            .await
            .expect_err("crlf injected");
        assert!(
            matches!(error_crlf, CecError::Rejected(_)),
            "{error_crlf:?}"
        );
    }

    #[tokio::test]
    async fn control_characters_and_null_bytes_are_rejected() {
        let transport = ClientTransport::new("definitely-not-a-cec-binary-xyz");
        let error_null = transport
            .send("on 0\0")
            .await
            .expect_err("null byte injected");
        assert!(
            matches!(error_null, CecError::Rejected(_)),
            "{error_null:?}"
        );

        let error_ctrl = transport
            .send("on 0\x1b[31m")
            .await
            .expect_err("control char injected");
        assert!(
            matches!(error_ctrl, CecError::Rejected(_)),
            "{error_ctrl:?}"
        );
    }

    #[tokio::test]
    async fn empty_or_whitespace_commands_are_rejected() {
        let transport = ClientTransport::new("definitely-not-a-cec-binary-xyz");
        let error_empty = transport.send("").await.expect_err("empty command");
        assert!(
            matches!(error_empty, CecError::Rejected(_)),
            "{error_empty:?}"
        );

        let error_ws = transport
            .send("   \t  ")
            .await
            .expect_err("whitespace command");
        assert!(matches!(error_ws, CecError::Rejected(_)), "{error_ws:?}");
    }

    #[tokio::test]
    async fn persistent_transport_missing_binary_is_unavailable() {
        let transport = PersistentClientTransport::new("definitely-not-a-cec-binary-xyz");
        let error = transport.send("on 0").await.expect_err("missing binary");
        assert!(matches!(error, CecError::Unavailable(_)), "{error:?}");
    }

    #[tokio::test]
    async fn malformed_cec_commands_are_rejected() {
        let transport = ClientTransport::new("definitely-not-a-cec-binary-xyz");
        let error_on = transport
            .send("on invalid_target")
            .await
            .expect_err("invalid on target");
        assert!(matches!(error_on, CecError::Rejected(_)), "{error_on:?}");

        let error_st = transport
            .send("standby invalid_target")
            .await
            .expect_err("invalid standby target");
        assert!(matches!(error_st, CecError::Rejected(_)), "{error_st:?}");
    }

    #[tokio::test]
    async fn persistent_transport_rejects_invalid_commands() {
        let transport = PersistentClientTransport::new("definitely-not-a-cec-binary-xyz");
        let error_newline = transport
            .send("on 0\nstandby 0")
            .await
            .expect_err("newline");
        assert!(matches!(error_newline, CecError::Rejected(_)));

        let error_null = transport.send("on 0\0").await.expect_err("null byte");
        assert!(matches!(error_null, CecError::Rejected(_)));

        let error_empty = transport.send("").await.expect_err("empty");
        assert!(matches!(error_empty, CecError::Rejected(_)));

        let error_on = transport
            .send("on bad_target")
            .await
            .expect_err("malformed command");
        assert!(matches!(error_on, CecError::Rejected(_)));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn persistent_transport_reuses_process_session() {
        use std::io::Write;
        let tmp_dir = std::env::var_os("CARGO_TARGET_TMPDIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let sh_path = tmp_dir.join("mock_cec_client.sh");

        let script = "#!/usr/bin/bash\nif [ -z \"$STDBUF_SET\" ]; then export STDBUF_SET=1; exec stdbuf -oL \"$0\" \"$@\"; fi\nwhile read -r line; do echo \"TRAFFIC: executed $line\"; done\n";
        let mut f = std::fs::File::create(&sh_path).unwrap();
        f.write_all(script.as_bytes()).unwrap();
        f.sync_all().unwrap();
        drop(f);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&sh_path).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&sh_path, perms).unwrap();
        }

        let transport = PersistentClientTransport::new(sh_path.to_str().unwrap())
            .with_read_timeout(Duration::from_millis(500));

        let res1 = transport.send("on 0").await.unwrap();
        assert!(res1.contains("TRAFFIC: executed on 0"), "{res1}");

        let res2 = transport.send("standby 0").await.unwrap();
        assert!(res2.contains("TRAFFIC: executed standby 0"), "{res2}");

        let _ = std::fs::remove_file(sh_path);
    }
}
