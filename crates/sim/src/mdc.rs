//! Mock Samsung MDC TCP protocol server.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

use tv_rs_mdc::protocol::{commands, ACK, HEADER, NAK, RESPONSE_COMMAND};

/// Simulated state for an MDC display.
#[derive(Debug, Clone)]
pub struct MdcDeviceState {
    /// Power state (1 = on, 0 = off).
    pub power: u8,
    /// Volume level (0..100).
    pub volume: u8,
    /// Mute state (1 = muted, 0 = unmuted).
    pub mute: u8,
    /// Active input code (e.g. 0x21 = HDMI1).
    pub input: u8,
    /// Brightness level (0..100).
    pub brightness: u8,
    /// Panel key lock (1 = locked, 0 = unlocked).
    pub panel_lock: u8,
}

impl Default for MdcDeviceState {
    fn default() -> Self {
        Self {
            power: 1,
            volume: 20,
            mute: 0,
            input: 0x21,
            brightness: 50,
            panel_lock: 0,
        }
    }
}

/// Mock MDC TCP Server.
pub struct MockMdcServer {
    addr: SocketAddr,
    state: Arc<Mutex<MdcDeviceState>>,
    task: JoinHandle<()>,
}

impl MockMdcServer {
    /// Starts a mock MDC server listening on `127.0.0.1:0` (ephemeral port).
    pub async fn start() -> Result<Self, std::io::Error> {
        Self::start_on("127.0.0.1:0").await
    }

    /// Starts a mock MDC server on the specified address.
    pub async fn start_on(bind_addr: &str) -> Result<Self, std::io::Error> {
        let listener = TcpListener::bind(bind_addr).await?;
        let addr = listener.local_addr()?;
        let state = Arc::new(Mutex::new(MdcDeviceState::default()));

        let task_state = Arc::clone(&state);
        let task = tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };

                let conn_state = Arc::clone(&task_state);
                tokio::spawn(async move {
                    let mut buf = [0u8; 512];
                    loop {
                        let n = match socket.read(&mut buf).await {
                            Ok(0) => break,
                            Ok(n) => n,
                            Err(_) => break,
                        };

                        let requests = parse_mdc_requests(&buf[..n]);
                        for req in requests {
                            let response_bytes = handle_mdc_request(&req, &conn_state);
                            if socket.write_all(&response_bytes).await.is_err() {
                                break;
                            }
                        }
                    }
                });
            }
        });

        Ok(Self { addr, state, task })
    }

    /// Returns the bound local socket address.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Returns the server's TCP port.
    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    /// Access the shared device state snapshot.
    pub fn state(&self) -> MdcDeviceState {
        self.state.lock().unwrap().clone()
    }

    /// Shuts down the mock MDC server.
    pub fn shutdown(self) {
        self.task.abort();
    }
}

impl Drop for MockMdcServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[derive(Debug)]
struct RawMdcRequest {
    command: u8,
    display_id: u8,
    data: Vec<u8>,
    checksum_valid: bool,
}

fn parse_mdc_requests(bytes: &[u8]) -> Vec<RawMdcRequest> {
    let mut out = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] != HEADER {
            i += 1;
            continue;
        }

        if i + 5 > bytes.len() {
            break;
        }

        let command = bytes[i + 1];
        let display_id = bytes[i + 2];
        let length = bytes[i + 3] as usize;

        if i + 5 + length > bytes.len() {
            break;
        }

        let data = bytes[i + 4..i + 4 + length].to_vec();
        let checksum = bytes[i + 4 + length];

        let mut sum = u16::from(command) + u16::from(display_id) + u16::from(bytes[i + 3]);
        for b in &data {
            sum += u16::from(*b);
        }
        let actual_checksum = (sum & 0xFF) as u8;

        out.push(RawMdcRequest {
            command,
            display_id,
            data,
            checksum_valid: checksum == actual_checksum,
        });

        i += 5 + length;
    }

    out
}

fn handle_mdc_request(req: &RawMdcRequest, state: &Arc<Mutex<MdcDeviceState>>) -> Vec<u8> {
    if !req.checksum_valid {
        return build_mdc_response(req.display_id, NAK, req.command, &[0x02]);
    }

    let mut lock = state.lock().unwrap();

    match req.command {
        commands::POWER => {
            if req.data.is_empty() {
                build_mdc_response(req.display_id, ACK, req.command, &[lock.power])
            } else {
                lock.power = req.data[0];
                build_mdc_response(req.display_id, ACK, req.command, &[lock.power])
            }
        }
        commands::VOLUME => {
            if req.data.is_empty() {
                build_mdc_response(req.display_id, ACK, req.command, &[lock.volume])
            } else {
                lock.volume = req.data[0];
                build_mdc_response(req.display_id, ACK, req.command, &[lock.volume])
            }
        }
        commands::MUTE => {
            if req.data.is_empty() {
                build_mdc_response(req.display_id, ACK, req.command, &[lock.mute])
            } else {
                lock.mute = req.data[0];
                build_mdc_response(req.display_id, ACK, req.command, &[lock.mute])
            }
        }
        commands::INPUT => {
            if req.data.is_empty() {
                build_mdc_response(req.display_id, ACK, req.command, &[lock.input])
            } else {
                lock.input = req.data[0];
                build_mdc_response(req.display_id, ACK, req.command, &[lock.input])
            }
        }
        commands::BRIGHTNESS => {
            if req.data.is_empty() {
                build_mdc_response(req.display_id, ACK, req.command, &[lock.brightness])
            } else {
                lock.brightness = req.data[0];
                build_mdc_response(req.display_id, ACK, req.command, &[lock.brightness])
            }
        }
        commands::PANEL_LOCK => {
            if req.data.is_empty() {
                build_mdc_response(req.display_id, ACK, req.command, &[lock.panel_lock])
            } else {
                lock.panel_lock = req.data[0];
                build_mdc_response(req.display_id, ACK, req.command, &[lock.panel_lock])
            }
        }
        commands::STATUS => {
            let body = vec![
                lock.power,
                lock.volume,
                lock.mute,
                lock.input,
                1, // aspect
                0, // n_err
                0, // rsv
            ];
            build_mdc_response(req.display_id, ACK, req.command, &body)
        }
        _ => build_mdc_response(req.display_id, ACK, req.command, &req.data),
    }
}

fn build_mdc_response(display_id: u8, ack: u8, command: u8, data: &[u8]) -> Vec<u8> {
    let mut body = vec![ack, command];
    body.extend_from_slice(data);

    let mut frame = vec![HEADER, RESPONSE_COMMAND, display_id, body.len() as u8];
    frame.extend_from_slice(&body);

    let checksum = frame[1..]
        .iter()
        .fold(0u16, |sum, byte| sum + u16::from(*byte))
        & 0xFF;

    frame.push(checksum as u8);
    frame
}
