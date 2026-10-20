//! The TCP MDC client and its shared-connection wall support.

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio::time::timeout;

use crate::error::MdcError;
use crate::payload::Status;
use crate::protocol::{
    commands, Frame, Response, HEADER, MDC_TCP_PORT, RESPONSE_COMMAND, TERMINATOR,
};
use crate::types::{DisplayId, InputSource, MdcPower};

/// Connection and framing options for [`MdcClient`].
#[derive(Debug, Clone)]
pub struct MdcOptions {
    /// TCP port. The MDC default is [`MDC_TCP_PORT`].
    pub port: u16,
    /// Connect and per-response timeout.
    pub timeout: Duration,
    /// Append `0x0D` after each frame. Commercial displays usually accept the
    /// bare frame; some TV firmware requires the carriage return.
    pub line_terminator: bool,
}

impl Default for MdcOptions {
    fn default() -> Self {
        Self {
            port: MDC_TCP_PORT,
            timeout: Duration::from_secs(5),
            line_terminator: false,
        }
    }
}

impl MdcOptions {
    /// Enables or disables the trailing carriage return.
    #[must_use]
    pub fn with_line_terminator(mut self, enabled: bool) -> Self {
        self.line_terminator = enabled;
        self
    }
}

struct Inner {
    host: String,
    options: MdcOptions,
    socket: Mutex<Option<TcpStream>>,
}

/// A Samsung MDC connection addressed to one display.
///
/// Clones share one socket. Use [`MdcClient::display`] to create another
/// addressable handle for a video wall without opening a second connection:
///
/// ```no_run
/// use tv_rs_mdc::{DisplayId, MdcClient};
///
/// # async fn run() -> Result<(), tv_rs_mdc::MdcError> {
/// let wall = MdcClient::new("192.168.1.20");
/// let left = wall.display(DisplayId::new(1)?);
/// let right = wall.display(DisplayId::new(2)?);
/// left.power_on().await?;
/// right.power_on().await?;
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct MdcClient {
    inner: Arc<Inner>,
    display_id: DisplayId,
}

impl fmt::Debug for MdcClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MdcClient")
            .field("host", &self.inner.host)
            .field("port", &self.inner.options.port)
            .field("display_id", &self.display_id)
            .finish()
    }
}

impl MdcClient {
    /// Creates a client for display `0` on the default MDC port.
    #[must_use]
    pub fn new(host: impl Into<String>) -> Self {
        Self::with_options(host, MdcOptions::default())
    }

    /// Creates a client with explicit connection options.
    #[must_use]
    pub fn with_options(host: impl Into<String>, options: MdcOptions) -> Self {
        Self {
            inner: Arc::new(Inner {
                host: host.into(),
                options,
                socket: Mutex::new(None),
            }),
            display_id: DisplayId::DIRECT,
        }
    }

    /// Returns a handle for another display sharing this socket.
    #[must_use]
    pub fn display(&self, display_id: DisplayId) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            display_id,
        }
    }

    /// Returns a broadcast handle. Broadcast commands have no acknowledgement;
    /// use [`MdcClient::send_only`] with them.
    #[must_use]
    pub fn broadcast(&self) -> Self {
        self.display(DisplayId::BROADCAST)
    }

    /// The address this handle targets.
    #[must_use]
    pub fn display_id(&self) -> DisplayId {
        self.display_id
    }

    /// The configured host.
    #[must_use]
    pub fn host(&self) -> &str {
        &self.inner.host
    }

    /// The configured port.
    #[must_use]
    pub fn port(&self) -> u16 {
        self.inner.options.port
    }

    /// Opens the socket if it is not already open.
    ///
    /// # Errors
    ///
    /// Returns [`MdcError::Timeout`] or [`MdcError::Io`] when the display is
    /// unreachable.
    pub async fn connect(&self) -> Result<(), MdcError> {
        let mut guard = self.inner.socket.lock().await;
        if guard.is_none() {
            *guard = Some(self.connect_socket().await?);
        }
        Ok(())
    }

    /// Closes the socket. The next command reconnects.
    pub async fn disconnect(&self) {
        let mut guard = self.inner.socket.lock().await;
        if let Some(mut socket) = guard.take() {
            let _ = socket.shutdown().await;
        }
    }

    /// Whether a socket is currently held.
    pub async fn is_connected(&self) -> bool {
        self.inner.socket.lock().await.is_some()
    }

    /// Sends a command and waits for its acknowledgement.
    ///
    /// Commands are serialized on the shared socket. On a timeout or malformed
    /// response the socket is dropped so the next call starts from a known state;
    /// a `NAK` keeps it open because the wire is still in sync.
    ///
    /// # Errors
    ///
    /// Returns [`MdcError::BroadcastRequiresSendOnly`] for display id `0xFE`,
    /// [`MdcError::Nak`] when the display rejects the command, and transport or
    /// protocol errors otherwise.
    pub async fn request(&self, command: u8, data: &[u8]) -> Result<Response, MdcError> {
        if self.display_id.is_broadcast() {
            return Err(MdcError::BroadcastRequiresSendOnly);
        }

        let frame = Frame::new(command, self.display_id, data.to_vec());
        let bytes = frame.encode()?;

        let mut guard = self.inner.socket.lock().await;
        if guard.is_none() {
            *guard = Some(self.connect_socket().await?);
        }

        let result = {
            let socket = guard
                .as_mut()
                .ok_or_else(|| MdcError::Protocol("connection state disappeared".to_string()))?;
            match write_frame(
                socket,
                &bytes,
                self.inner.options.line_terminator,
                self.inner.options.timeout,
            )
            .await
            {
                Ok(()) => {
                    read_response(socket, self.display_id, command, self.inner.options.timeout)
                        .await
                }
                Err(error) => Err(error),
            }
        };

        let keep_open = matches!(result.as_ref(), Err(MdcError::Nak { .. }));
        if result.is_err() && !keep_open {
            *guard = None;
        }

        let response = result?;
        if response.acknowledged {
            Ok(response)
        } else {
            Err(MdcError::Nak {
                command: response.command,
                code: response.data.first().copied().unwrap_or(0),
            })
        }
    }

    /// Sends a command without waiting for a reply.
    ///
    /// This is the only valid operation for a broadcast handle. It is also useful
    /// for fire-and-forget commands on very slow walls.
    ///
    /// # Errors
    ///
    /// Returns a transport or framing error.
    pub async fn send_only(&self, command: u8, data: &[u8]) -> Result<(), MdcError> {
        let frame = Frame::new(command, self.display_id, data.to_vec());
        let bytes = frame.encode()?;

        let mut guard = self.inner.socket.lock().await;
        if guard.is_none() {
            *guard = Some(self.connect_socket().await?);
        }
        let socket = guard
            .as_mut()
            .ok_or_else(|| MdcError::Protocol("connection state disappeared".to_string()))?;
        let result = write_frame(
            socket,
            &bytes,
            self.inner.options.line_terminator,
            self.inner.options.timeout,
        )
        .await;
        if result.is_err() {
            *guard = None;
        }
        result
    }

    /// Reads power, volume, mute, input and aspect in one round trip.
    ///
    /// # Errors
    ///
    /// Returns a transport, protocol or `NAK` error.
    pub async fn status(&self) -> Result<Status, MdcError> {
        let data = self.query(commands::STATUS).await?;
        Status::from_payload(&data)
    }

    /// Reads the power state.
    ///
    /// # Errors
    ///
    /// Returns a transport, protocol or `NAK` error.
    pub async fn power(&self) -> Result<MdcPower, MdcError> {
        let data = self.query(commands::POWER).await?;
        let code = *data
            .first()
            .ok_or_else(|| MdcError::Protocol("power reply has no value".to_string()))?;
        MdcPower::from_code(code)
            .ok_or_else(|| MdcError::Protocol(format!("unknown power code 0x{code:02X}")))
    }

    /// Sets the power state.
    ///
    /// # Errors
    ///
    /// Returns a transport, protocol or `NAK` error.
    pub async fn set_power(&self, power: MdcPower) -> Result<(), MdcError> {
        let response = self.request(commands::POWER, &[power.code()]).await?;
        expect_echo(&response.data, power.code(), "power")
    }

    /// Powers the display on.
    ///
    /// # Errors
    ///
    /// Returns a transport, protocol or `NAK` error.
    pub async fn power_on(&self) -> Result<(), MdcError> {
        self.set_power(MdcPower::On).await
    }

    /// Puts the display in standby/off.
    ///
    /// # Errors
    ///
    /// Returns a transport, protocol or `NAK` error.
    pub async fn power_off(&self) -> Result<(), MdcError> {
        self.set_power(MdcPower::Off).await
    }

    /// Reboots the display.
    ///
    /// # Errors
    ///
    /// Returns a transport, protocol or `NAK` error.
    pub async fn reboot(&self) -> Result<(), MdcError> {
        self.set_power(MdcPower::Reboot).await
    }

    /// Reads the volume. `None` means the panel has no audio.
    ///
    /// # Errors
    ///
    /// Returns a transport, protocol or `NAK` error.
    pub async fn volume(&self) -> Result<Option<u8>, MdcError> {
        let data = self.query(commands::VOLUME).await?;
        level_from_payload(&data, "volume", 100)
    }

    /// Sets the volume, 0-100.
    ///
    /// # Errors
    ///
    /// Returns [`MdcError::InvalidValue`] for a value above 100, a transport
    /// error, a protocol error or a `NAK`.
    pub async fn set_volume(&self, level: u8) -> Result<(), MdcError> {
        validate_level("volume", level, 100)?;
        let response = self.request(commands::VOLUME, &[level]).await?;
        expect_echo(&response.data, level, "volume")
    }

    /// Reads the mute state. `None` means the panel has no audio.
    ///
    /// # Errors
    ///
    /// Returns a transport, protocol or `NAK` error.
    pub async fn muted(&self) -> Result<Option<bool>, MdcError> {
        let data = self.query(commands::MUTE).await?;
        match data.first().copied() {
            Some(0x00) => Ok(Some(false)),
            Some(0x01) => Ok(Some(true)),
            Some(0xFF) => Ok(None),
            Some(code) => Err(MdcError::Protocol(format!(
                "mute reply 0x{code:02X} is not 0, 1 or 0xFF"
            ))),
            None => Err(MdcError::Protocol("mute reply has no value".to_string())),
        }
    }

    /// Mutes or unmutes.
    ///
    /// # Errors
    ///
    /// Returns a transport, protocol or `NAK` error.
    pub async fn set_mute(&self, muted: bool) -> Result<(), MdcError> {
        let value = u8::from(muted);
        let response = self.request(commands::MUTE, &[value]).await?;
        expect_echo(&response.data, value, "mute")
    }

    /// Reads the active input.
    ///
    /// # Errors
    ///
    /// Returns a transport, protocol or `NAK` error.
    pub async fn active_input(&self) -> Result<Option<InputSource>, MdcError> {
        let data = self.query(commands::INPUT).await?;
        let code = *data
            .first()
            .ok_or_else(|| MdcError::Protocol("input reply has no value".to_string()))?;
        Ok(InputSource::from_code(code))
    }

    /// Switches to an input source.
    ///
    /// # Errors
    ///
    /// Returns a transport, protocol or `NAK` error.
    pub async fn set_input(&self, input: InputSource) -> Result<(), MdcError> {
        let response = self.request(commands::INPUT, &[input.code()]).await?;
        expect_echo(&response.data, input.code(), "input")
    }

    /// Reads the brightness. `None` means the display does not report it.
    ///
    /// # Errors
    ///
    /// Returns a transport, protocol or `NAK` error.
    pub async fn brightness(&self) -> Result<Option<u8>, MdcError> {
        let data = self.query(commands::BRIGHTNESS).await?;
        level_from_payload(&data, "brightness", 100)
    }

    /// Sets the brightness, 0-100.
    ///
    /// # Errors
    ///
    /// Returns [`MdcError::InvalidValue`] for a value above 100, a transport
    /// error, a protocol error or a `NAK`.
    pub async fn set_brightness(&self, level: u8) -> Result<(), MdcError> {
        validate_level("brightness", level, 100)?;
        let response = self.request(commands::BRIGHTNESS, &[level]).await?;
        expect_echo(&response.data, level, "brightness")
    }

    async fn query(&self, command: u8) -> Result<Vec<u8>, MdcError> {
        Ok(self.request(command, &[]).await?.data)
    }

    async fn connect_socket(&self) -> Result<TcpStream, MdcError> {
        let target = (self.inner.host.as_str(), self.inner.options.port);
        let socket = timeout(self.inner.options.timeout, TcpStream::connect(target))
            .await
            .map_err(|_| MdcError::Timeout)??;
        let _ = socket.set_nodelay(true);
        Ok(socket)
    }
}

async fn write_frame(
    socket: &mut TcpStream,
    bytes: &[u8],
    line_terminator: bool,
    budget: Duration,
) -> Result<(), MdcError> {
    let mut buffer = Vec::with_capacity(bytes.len() + if line_terminator { 1 } else { 0 });
    buffer.extend_from_slice(bytes);
    if line_terminator {
        buffer.push(TERMINATOR);
    }
    timeout(budget, socket.write_all(&buffer))
        .await
        .map_err(|_| MdcError::Timeout)??;
    Ok(())
}

async fn read_response(
    socket: &mut TcpStream,
    expected_id: DisplayId,
    expected_command: u8,
    budget: Duration,
) -> Result<Response, MdcError> {
    let response = timeout(budget, read_response_frame(socket))
        .await
        .map_err(|_| MdcError::Timeout)??;
    if response.display_id != expected_id {
        return Err(MdcError::Protocol(format!(
            "reply came from display {}; expected {expected_id}",
            response.display_id
        )));
    }
    if response.command != expected_command {
        return Err(MdcError::Protocol(format!(
            "reply echoes command 0x{:02X}; expected 0x{expected_command:02X}",
            response.command
        )));
    }
    Ok(response)
}

async fn read_response_frame(socket: &mut TcpStream) -> Result<Response, MdcError> {
    let mut header = [0u8; 4];
    let mut discarded = 0usize;
    loop {
        let byte = socket.read_u8().await?;
        if byte == HEADER {
            header[0] = byte;
            break;
        }
        discarded += 1;
        if discarded > 64 {
            return Err(MdcError::Protocol(
                "discarded 64 bytes without finding an MDC header".to_string(),
            ));
        }
    }
    socket.read_exact(&mut header[1..]).await?;
    if header[1] != RESPONSE_COMMAND {
        return Err(MdcError::Protocol(format!(
            "reply command is 0x{:02X}, not 0xFF",
            header[1]
        )));
    }

    let length = usize::from(header[3]);
    let mut body = vec![0u8; length + 1];
    socket.read_exact(&mut body).await?;

    let mut frame = Vec::with_capacity(header.len() + body.len());
    frame.extend_from_slice(&header);
    frame.extend_from_slice(&body);
    Response::parse(&frame)
}

fn validate_level(field: &'static str, value: u8, max: u8) -> Result<(), MdcError> {
    if value > max {
        return Err(MdcError::InvalidValue {
            field,
            value: u16::from(value),
            max: u16::from(max),
        });
    }
    Ok(())
}

fn level_from_payload(data: &[u8], field: &'static str, max: u8) -> Result<Option<u8>, MdcError> {
    let value = *data
        .first()
        .ok_or_else(|| MdcError::Protocol(format!("{field} reply has no value")))?;
    if value == 0xFF {
        return Ok(None);
    }
    validate_level(field, value, max)?;
    Ok(Some(value))
}

fn expect_echo(data: &[u8], expected: u8, field: &'static str) -> Result<(), MdcError> {
    if let Some(actual) = data.first() {
        if *actual != expected {
            return Err(MdcError::Protocol(format!(
                "{field} acknowledgement echoes 0x{actual:02X}; expected 0x{expected:02X}"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{level_from_payload, validate_level, MdcOptions};
    use crate::error::MdcError;

    #[test]
    fn defaults_are_the_documented_mdc_port_and_no_terminator() {
        let options = MdcOptions::default();
        assert_eq!(options.port, 1515);
        assert_eq!(options.timeout.as_secs(), 5);
        assert!(!options.line_terminator);
    }

    #[test]
    fn levels_are_checked_and_0xff_means_unavailable() {
        assert!(validate_level("volume", 100, 100).is_ok());
        assert!(matches!(
            validate_level("volume", 101, 100),
            Err(MdcError::InvalidValue { .. })
        ));
        assert_eq!(level_from_payload(&[0xFF], "volume", 100).unwrap(), None);
        assert_eq!(level_from_payload(&[50], "volume", 100).unwrap(), Some(50));
    }
}
