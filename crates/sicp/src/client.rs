//! The TCP/Serial Philips SICP client.

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio::time::timeout;

use crate::error::SicpError;
use crate::payload::Status;
use crate::protocol::{commands, control_types, Frame, Response, SICP_TCP_PORT};
use crate::transport::Transport;
use crate::types::{DisplayId, SicpInput, SicpPower};

#[cfg(feature = "serial")]
use tokio_serial::SerialPortBuilderExt;

/// Options for [`SicpClient`].
#[derive(Debug, Clone)]
pub struct SicpOptions {
    /// TCP port. Default is [`SICP_TCP_PORT`] (5000).
    pub port: u16,
    /// Connection and per-request timeout.
    pub timeout: Duration,
}

impl Default for SicpOptions {
    fn default() -> Self {
        Self {
            port: SICP_TCP_PORT,
            timeout: Duration::from_secs(5),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Link {
    Tcp,
    #[cfg(feature = "serial")]
    Serial {
        baud: u32,
    },
}

impl Link {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Tcp => "tcp",
            #[cfg(feature = "serial")]
            Self::Serial { .. } => "serial",
        }
    }
}

struct Inner {
    host: String,
    options: SicpOptions,
    link: Link,
    socket: Mutex<Option<Transport>>,
}

/// Client for communicating with Philips commercial displays via SICP.
#[derive(Clone)]
pub struct SicpClient {
    inner: Arc<Inner>,
    display_id: DisplayId,
}

impl fmt::Debug for SicpClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SicpClient")
            .field("host", &self.inner.host)
            .field("carrier", &self.inner.link.as_str())
            .field("port", &self.inner.options.port)
            .field("display_id", &self.display_id)
            .finish()
    }
}

impl SicpClient {
    /// Creates a client for display `1` on default TCP port 5000.
    #[must_use]
    pub fn new(host: impl Into<String>) -> Self {
        Self::with_options(host, SicpOptions::default())
    }

    /// Creates a client with custom options.
    #[must_use]
    pub fn with_options(host: impl Into<String>, options: SicpOptions) -> Self {
        Self::with_link(host, options, Link::Tcp)
    }

    /// Creates a client over RS-232 serial (`serial` feature).
    #[cfg(feature = "serial")]
    #[must_use]
    pub fn serial(path: impl Into<String>, baud: u32) -> Self {
        Self::with_link(path, SicpOptions::default(), Link::Serial { baud })
    }

    fn with_link(host: impl Into<String>, options: SicpOptions, link: Link) -> Self {
        Self {
            inner: Arc::new(Inner {
                host: host.into(),
                options,
                link,
                socket: Mutex::new(None),
            }),
            display_id: DisplayId::DIRECT,
        }
    }

    /// Sets the target display ID.
    #[must_use]
    pub fn display(&self, display_id: DisplayId) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            display_id,
        }
    }

    /// Returns a broadcast handle.
    #[must_use]
    pub fn broadcast(&self) -> Self {
        self.display(DisplayId::BROADCAST)
    }

    /// The display ID this handle targets.
    #[must_use]
    pub fn display_id(&self) -> DisplayId {
        self.display_id
    }

    /// The target host/address.
    #[must_use]
    pub fn host(&self) -> &str {
        &self.inner.host
    }

    /// The target TCP port.
    #[must_use]
    pub fn port(&self) -> u16 {
        self.inner.options.port
    }

    /// Opens the connection if not already open.
    pub async fn connect(&self) -> Result<(), SicpError> {
        let mut guard = self.inner.socket.lock().await;
        if guard.is_none() {
            *guard = Some(self.connect_socket().await?);
        }
        Ok(())
    }

    /// Disconnects the socket.
    pub async fn disconnect(&self) {
        let mut guard = self.inner.socket.lock().await;
        if let Some(mut socket) = guard.take() {
            let _ = socket.shutdown().await;
        }
    }

    /// Checks if socket is connected.
    pub async fn is_connected(&self) -> bool {
        self.inner.socket.lock().await.is_some()
    }

    /// Sends a SICP frame and awaits its response.
    pub async fn request(
        &self,
        control_type: u8,
        command: u8,
        data: &[u8],
    ) -> Result<Response, SicpError> {
        if self.display_id.is_broadcast() {
            return Err(SicpError::BroadcastRequiresSendOnly);
        }

        let frame = Frame::new(self.display_id, control_type, command, data.to_vec());
        let bytes = frame.encode()?;

        let mut guard = self.inner.socket.lock().await;
        if guard.is_none() {
            *guard = Some(self.connect_socket().await?);
        }

        let result = {
            let socket = guard
                .as_mut()
                .ok_or_else(|| SicpError::Protocol("connection state disappeared".to_string()))?;
            match write_frame(socket, &bytes, self.inner.options.timeout).await {
                Ok(()) => {
                    read_response(socket, self.display_id, command, self.inner.options.timeout)
                        .await
                }
                Err(err) => Err(err),
            }
        };

        let keep_open = matches!(result.as_ref(), Err(SicpError::Nak { .. }));
        if result.is_err() && !keep_open {
            *guard = None;
        }

        let response = result?;
        if response.acknowledged {
            Ok(response)
        } else {
            Err(SicpError::Nak {
                command: response.command,
                code: response.data.first().copied().unwrap_or(0),
            })
        }
    }

    /// Sends a SICP frame without waiting for a reply.
    pub async fn send_only(
        &self,
        control_type: u8,
        command: u8,
        data: &[u8],
    ) -> Result<(), SicpError> {
        let frame = Frame::new(self.display_id, control_type, command, data.to_vec());
        let bytes = frame.encode()?;

        let mut guard = self.inner.socket.lock().await;
        if guard.is_none() {
            *guard = Some(self.connect_socket().await?);
        }
        let socket = guard
            .as_mut()
            .ok_or_else(|| SicpError::Protocol("connection state disappeared".to_string()))?;
        let result = write_frame(socket, &bytes, self.inner.options.timeout).await;
        if result.is_err() {
            *guard = None;
        }
        result
    }

    /// Queries current power state.
    pub async fn power(&self) -> Result<SicpPower, SicpError> {
        let response = self
            .request(control_types::GET, commands::POWER, &[])
            .await?;
        let code = *response
            .data
            .first()
            .ok_or_else(|| SicpError::Protocol("power reply carried no value".to_string()))?;
        SicpPower::from_code(code)
            .ok_or_else(|| SicpError::Protocol(format!("unknown power code 0x{code:02X}")))
    }

    /// Sets power state.
    pub async fn set_power(&self, power: SicpPower) -> Result<(), SicpError> {
        let response = self
            .request(control_types::SET, commands::POWER, &[power.code()])
            .await?;
        if let Some(&echo) = response.data.first() {
            if echo != power.code() {
                return Err(SicpError::Protocol(format!(
                    "power set echoed 0x{echo:02X}; expected 0x{:02X}",
                    power.code()
                )));
            }
        }
        Ok(())
    }

    /// Powers on the display.
    pub async fn power_on(&self) -> Result<(), SicpError> {
        self.set_power(SicpPower::On).await
    }

    /// Powers off the display.
    pub async fn power_off(&self) -> Result<(), SicpError> {
        self.set_power(SicpPower::Off).await
    }

    /// Queries active input source.
    pub async fn active_input(&self) -> Result<Option<SicpInput>, SicpError> {
        let response = self
            .request(control_types::GET, commands::INPUT, &[])
            .await?;
        let code = *response
            .data
            .first()
            .ok_or_else(|| SicpError::Protocol("input reply carried no value".to_string()))?;
        Ok(Some(SicpInput::from_code(code)))
    }

    /// Sets active input source.
    pub async fn set_input(&self, input: SicpInput) -> Result<(), SicpError> {
        let response = self
            .request(control_types::SET, commands::INPUT, &[input.code()])
            .await?;
        if let Some(&echo) = response.data.first() {
            if echo != input.code() {
                return Err(SicpError::Protocol(format!(
                    "input set echoed 0x{echo:02X}; expected 0x{:02X}",
                    input.code()
                )));
            }
        }
        Ok(())
    }

    /// Queries volume level (0–100).
    pub async fn volume(&self) -> Result<Option<u8>, SicpError> {
        let response = self
            .request(control_types::GET, commands::VOLUME, &[])
            .await?;
        let level = *response
            .data
            .first()
            .ok_or_else(|| SicpError::Protocol("volume reply carried no value".to_string()))?;
        if level > 100 {
            return Err(SicpError::InvalidValue {
                field: "volume",
                value: u16::from(level),
                max: 100,
            });
        }
        Ok(Some(level))
    }

    /// Sets volume level (0–100).
    pub async fn set_volume(&self, level: u8) -> Result<(), SicpError> {
        if level > 100 {
            return Err(SicpError::InvalidValue {
                field: "volume",
                value: u16::from(level),
                max: 100,
            });
        }
        let response = self
            .request(control_types::SET, commands::VOLUME, &[level])
            .await?;
        if let Some(&echo) = response.data.first() {
            if echo != level {
                return Err(SicpError::Protocol(format!(
                    "volume set echoed {echo}; expected {level}"
                )));
            }
        }
        Ok(())
    }

    /// Reads full status including telemetry.
    pub async fn status(&self) -> Result<Status, SicpError> {
        let power = self.power().await?;
        let volume = self.volume().await?.unwrap_or(0);
        let input = self.active_input().await?.unwrap_or(SicpInput::Hdmi1);
        let tele_res = self
            .request(control_types::GET, commands::TEMPERATURE_STATUS, &[])
            .await?;
        Status::from_telemetry_payload(power, volume, input, &tele_res.data)
    }

    async fn connect_socket(&self) -> Result<Transport, SicpError> {
        match self.inner.link {
            Link::Tcp => {
                let target = (self.inner.host.as_str(), self.inner.options.port);
                let socket = timeout(self.inner.options.timeout, TcpStream::connect(target))
                    .await
                    .map_err(|_| SicpError::Timeout)??;
                let _ = socket.set_nodelay(true);
                Ok(Transport::Tcp(socket))
            }
            #[cfg(feature = "serial")]
            Link::Serial { baud } => {
                let stream = tokio_serial::new(self.inner.host.clone(), baud)
                    .open_native_async()
                    .map_err(|err| SicpError::Io(std::io::Error::other(err.to_string())))?;
                Ok(Transport::Serial(Box::new(stream)))
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn with_duplex(stream: tokio::io::DuplexStream) -> Self {
        let client = Self::with_link("duplex", SicpOptions::default(), Link::Tcp);
        client
            .inner
            .socket
            .try_lock()
            .expect("fresh client")
            .replace(Transport::Duplex(Box::new(stream)));
        client
    }
}

async fn write_frame(
    socket: &mut Transport,
    bytes: &[u8],
    budget: Duration,
) -> Result<(), SicpError> {
    timeout(budget, socket.write_all(bytes))
        .await
        .map_err(|_| SicpError::Timeout)??;
    Ok(())
}

async fn read_response(
    socket: &mut Transport,
    expected_id: DisplayId,
    expected_command: u8,
    budget: Duration,
) -> Result<Response, SicpError> {
    let response = timeout(budget, read_response_frame(socket))
        .await
        .map_err(|_| SicpError::Timeout)??;

    if response.display_id != expected_id && !expected_id.is_broadcast() {
        return Err(SicpError::Protocol(format!(
            "reply came from display {}; expected {expected_id}",
            response.display_id
        )));
    }
    if response.command != expected_command {
        return Err(SicpError::Protocol(format!(
            "reply echoes command 0x{:02X}; expected 0x{expected_command:02X}",
            response.command
        )));
    }
    Ok(response)
}

async fn read_response_frame(socket: &mut Transport) -> Result<Response, SicpError> {
    let size_byte = socket.read_u8().await?;
    let size = size_byte as usize;
    if size < 5 {
        return Err(SicpError::Protocol(format!(
            "frame declared size {size} is less than minimum 5"
        )));
    }

    let mut buf = vec![0u8; size];
    buf[0] = size_byte;
    socket.read_exact(&mut buf[1..]).await?;

    Response::parse(&buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn request_response_duplex_cycle() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let (client_side, mut panel_side) = tokio::io::duplex(256);
        let client = SicpClient::with_duplex(client_side);

        let panel_task = tokio::spawn(async move {
            let mut req = [0u8; 5];
            panel_side.read_exact(&mut req).await.unwrap();
            assert_eq!(
                req,
                [5, 1, control_types::GET, commands::POWER, 5 ^ 1 ^ 1 ^ 0x18]
            );

            let resp = vec![
                6,
                1,
                control_types::REPLY,
                commands::POWER,
                0x02,
                6 ^ 1 ^ 2 ^ 0x18 ^ 2,
            ];
            panel_side.write_all(&resp).await.unwrap();
        });

        let power = client.power().await.unwrap();
        assert_eq!(power, SicpPower::On);
        panel_task.await.unwrap();
    }
}
