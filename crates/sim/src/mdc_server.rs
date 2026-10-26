//! Samsung MDC TCP hardware mock server for integration testing.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use tv_rs_mdc::protocol::{commands, ACK, HEADER, RESPONSE_COMMAND};

#[derive(Debug, Clone)]
struct DisplayState {
    power: u8,
    volume: u8,
    muted: u8,
    input: u8,
    brightness: u8,
    panel_lock: u8,
    temperature: u8,
}

impl Default for DisplayState {
    fn default() -> Self {
        Self {
            power: 1,       // On
            volume: 50,     // 50%
            muted: 0,       // Unmuted
            input: 0x14,    // HDMI1
            brightness: 50, // 50%
            panel_lock: 0,  // Unlocked
            temperature: 35,
        }
    }
}

/// A TCP mock server answering Samsung MDC frames.
pub struct MockMdcServer {
    local_addr: SocketAddr,
    shutdown_tx: Option<oneshot::Sender<()>>,
    server_task: Option<JoinHandle<()>>,
}

impl MockMdcServer {
    /// Starts the MDC mock server bound to `bind_addr` (e.g. `"127.0.0.1:0"`).
    pub async fn start(bind_addr: &str) -> Result<Self, std::io::Error> {
        let listener = TcpListener::bind(bind_addr).await?;
        let local_addr = listener.local_addr()?;
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();

        let displays: Arc<Mutex<HashMap<u8, DisplayState>>> = Arc::new(Mutex::new(HashMap::new()));

        let server_task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => break,
                    accept_res = listener.accept() => {
                        match accept_res {
                            Ok((mut socket, _)) => {
                                let displays = Arc::clone(&displays);
                                tokio::spawn(async move {
                                    while let Ok(header_byte) = socket.read_u8().await {
                                        if header_byte != HEADER {
                                            continue;
                                        }

                                        let command = match socket.read_u8().await {
                                            Ok(b) => b,
                                            Err(_) => break,
                                        };
                                        let display_id = match socket.read_u8().await {
                                            Ok(b) => b,
                                            Err(_) => break,
                                        };
                                        let len = match socket.read_u8().await {
                                            Ok(b) => b as usize,
                                            Err(_) => break,
                                        };

                                        let mut data = vec![0u8; len + 1];
                                        if socket.read_exact(&mut data).await.is_err() {
                                            break;
                                        }
                                        let payload = &data[..len];

                                        let mut resp_data = Vec::new();
                                        {
                                            let mut state_map = displays.lock().unwrap();
                                            let state = state_map.entry(display_id).or_default();

                                            resp_data.push(ACK);
                                            resp_data.push(command);

                                            match command {
                                                commands::STATUS => {
                                                    resp_data.push(state.power);
                                                    resp_data.push(state.volume);
                                                    resp_data.push(state.muted);
                                                    resp_data.push(state.input);
                                                    resp_data.push(0x21); // 16:9
                                                    resp_data.push(0);
                                                    resp_data.push(0);
                                                }
                                                commands::POWER => {
                                                    if payload.is_empty() {
                                                        resp_data.push(state.power);
                                                    } else {
                                                        state.power = payload[0];
                                                        resp_data.push(state.power);
                                                    }
                                                }
                                                commands::VOLUME => {
                                                    if payload.is_empty() {
                                                        resp_data.push(state.volume);
                                                    } else {
                                                        state.volume = payload[0];
                                                        resp_data.push(state.volume);
                                                    }
                                                }
                                                commands::MUTE => {
                                                    if payload.is_empty() {
                                                        resp_data.push(state.muted);
                                                    } else {
                                                        state.muted = payload[0];
                                                        resp_data.push(state.muted);
                                                    }
                                                }
                                                commands::INPUT => {
                                                    if payload.is_empty() {
                                                        resp_data.push(state.input);
                                                    } else {
                                                        state.input = payload[0];
                                                        resp_data.push(state.input);
                                                    }
                                                }
                                                commands::BRIGHTNESS => {
                                                    if payload.is_empty() {
                                                        resp_data.push(state.brightness);
                                                    } else {
                                                        state.brightness = payload[0];
                                                        resp_data.push(state.brightness);
                                                    }
                                                }
                                                commands::PANEL_LOCK => {
                                                    if payload.is_empty() {
                                                        resp_data.push(state.panel_lock);
                                                    } else {
                                                        state.panel_lock = payload[0];
                                                        resp_data.push(state.panel_lock);
                                                    }
                                                }
                                                commands::ERROR_STATUS => {
                                                    resp_data.push(0); // lamp ok
                                                    resp_data.push(state.temperature);
                                                    resp_data.push(0); // fan ok
                                                }
                                                _ => {
                                                    resp_data.push(0);
                                                }
                                            }
                                        }

                                        let mut frame = vec![HEADER, RESPONSE_COMMAND, display_id, resp_data.len() as u8];
                                        frame.extend_from_slice(&resp_data);
                                        let checksum = frame[1..].iter().fold(0u16, |acc, &b| acc + u16::from(b)) & 0xFF;
                                        frame.push(checksum as u8);

                                        if socket.write_all(&frame).await.is_err() {
                                            break;
                                        }
                                    }
                                });
                            }
                            Err(_) => break,
                        }
                    }
                }
            }
        });

        Ok(Self {
            local_addr,
            shutdown_tx: Some(shutdown_tx),
            server_task: Some(server_task),
        })
    }

    /// Bound local socket address.
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Shuts down the server.
    pub async fn shutdown(mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(task) = self.server_task.take() {
            let _ = task.await;
        }
    }
}

impl Drop for MockMdcServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tv_rs_mdc::{DisplayId, MdcClient};

    #[tokio::test]
    async fn mdc_server_answers_client_commands() {
        let server = MockMdcServer::start("127.0.0.1:0").await.unwrap();
        let client = MdcClient::with_options(
            server.local_addr().ip().to_string(),
            tv_rs_mdc::MdcOptions {
                port: server.local_addr().port(),
                ..Default::default()
            },
        )
        .display(DisplayId::new(1).unwrap());

        client.power_on().await.unwrap();
        let power = client.power().await.unwrap();
        assert_eq!(power, tv_rs_mdc::MdcPower::On);

        client.set_volume(75).await.unwrap();
        let vol = client.volume().await.unwrap();
        assert_eq!(vol, Some(75));

        server.shutdown().await;
    }
}
