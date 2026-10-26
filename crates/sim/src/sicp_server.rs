//! Philips SICP TCP hardware mock server for integration testing.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use tv_rs_sicp::protocol::{commands, control_types};

#[derive(Debug, Clone)]
struct DisplayState {
    power: u8,
    volume: u8,
    input: u8,
    temperature: i8,
    signal_present: u8,
    lamp_error: u8,
}

impl Default for DisplayState {
    fn default() -> Self {
        Self {
            power: 0x02, // On
            volume: 50,  // 50%
            input: 0x04, // HDMI1
            temperature: 35,
            signal_present: 1,
            lamp_error: 0,
        }
    }
}

/// A TCP mock server answering Philips SICP frames.
pub struct MockSicpServer {
    local_addr: SocketAddr,
    shutdown_tx: Option<oneshot::Sender<()>>,
    server_task: Option<JoinHandle<()>>,
}

impl MockSicpServer {
    /// Starts the SICP mock server bound to `bind_addr` (e.g. `"127.0.0.1:0"`).
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
                                    while let Ok(raw_size) = socket.read_u8().await {
                                        let size_byte = raw_size as usize;
                                        if !(5..=512).contains(&size_byte) {
                                            break;
                                        }

                                        let mut buf = vec![0u8; size_byte];
                                        buf[0] = raw_size;
                                        if socket.read_exact(&mut buf[1..size_byte]).await.is_err() {
                                            break;
                                        }

                                        let display_id = buf[1];
                                        let control_type = buf[2];
                                        let command = buf[3];
                                        let data = &buf[4..size_byte - 1];

                                        let mut resp_data = Vec::new();
                                        {
                                            let mut state_map = displays.lock().unwrap();
                                            let state = state_map.entry(display_id).or_default();

                                            match control_type {
                                                control_types::GET => match command {
                                                    commands::POWER => resp_data.push(state.power),
                                                    commands::VOLUME => resp_data.push(state.volume),
                                                    commands::INPUT => resp_data.push(state.input),
                                                    commands::TEMPERATURE_STATUS => {
                                                        resp_data.push(state.temperature as u8);
                                                        resp_data.push(state.signal_present);
                                                        resp_data.push(state.lamp_error);
                                                    }
                                                    _ => {}
                                                },
                                                control_types::SET => match command {
                                                    commands::POWER => {
                                                        if let Some(&val) = data.first() {
                                                            state.power = val;
                                                        }
                                                        resp_data.push(state.power);
                                                    }
                                                    commands::VOLUME => {
                                                        if let Some(&val) = data.first() {
                                                            state.volume = val;
                                                        }
                                                        resp_data.push(state.volume);
                                                    }
                                                    commands::INPUT => {
                                                        if let Some(&val) = data.first() {
                                                            state.input = val;
                                                        }
                                                        resp_data.push(state.input);
                                                    }
                                                    _ => {}
                                                },
                                                _ => {}
                                            }
                                        }

                                        let resp_size = (5 + resp_data.len()) as u8;
                                        let mut frame = vec![resp_size, display_id, control_types::REPLY, command];
                                        frame.extend_from_slice(&resp_data);
                                        let checksum = frame.iter().fold(0u8, |acc, &b| acc ^ b);
                                        frame.push(checksum);

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

impl Drop for MockSicpServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tv_rs_sicp::{DisplayId, SicpClient, SicpInput, SicpPower};

    #[tokio::test]
    async fn sicp_server_answers_client_commands() {
        let server = MockSicpServer::start("127.0.0.1:0").await.unwrap();
        let client = SicpClient::with_options(
            server.local_addr().ip().to_string(),
            tv_rs_sicp::SicpOptions {
                port: server.local_addr().port(),
                ..Default::default()
            },
        )
        .display(DisplayId::new(1).unwrap());

        let power = client.power().await.unwrap();
        assert_eq!(power, SicpPower::On);

        client.set_power(SicpPower::Standby).await.unwrap();
        let power_after = client.power().await.unwrap();
        assert_eq!(power_after, SicpPower::Standby);

        client.set_volume(80).await.unwrap();
        let vol = client.volume().await.unwrap();
        assert_eq!(vol, Some(80));

        client.set_input(SicpInput::Hdmi2).await.unwrap();
        let inp = client.active_input().await.unwrap();
        assert_eq!(inp, Some(SicpInput::Hdmi2));

        server.shutdown().await;
    }
}
