//! Mock Samsung Tizen WebSocket protocol server.

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;

/// Simulated state for a Tizen TV.
#[derive(Debug, Clone)]
pub struct TizenDeviceState {
    /// Pairing token.
    pub token: String,
    /// TV display name.
    pub name: String,
    /// Last keypress received (`DataOfCmd`).
    pub last_key: Option<String>,
    /// Last launched application ID.
    pub last_launched_app: Option<String>,
}

impl Default for TizenDeviceState {
    fn default() -> Self {
        Self {
            token: "sim-tizen-token-67890".to_string(),
            name: "Simulated Samsung TV".to_string(),
            last_key: None,
            last_launched_app: None,
        }
    }
}

/// Mock Tizen Remote Channel WebSocket Server.
pub struct MockTizenServer {
    addr: SocketAddr,
    state: Arc<Mutex<TizenDeviceState>>,
    task: JoinHandle<()>,
}

impl MockTizenServer {
    /// Starts a mock Tizen server on an ephemeral local port (`127.0.0.1:0`).
    pub async fn start() -> Result<Self, std::io::Error> {
        Self::start_on("127.0.0.1:0").await
    }

    /// Starts a mock Tizen server on `bind_addr`.
    pub async fn start_on(bind_addr: &str) -> Result<Self, std::io::Error> {
        let listener = TcpListener::bind(bind_addr).await?;
        let addr = listener.local_addr()?;
        let state = Arc::new(Mutex::new(TizenDeviceState::default()));

        let task_state = Arc::clone(&state);
        let task = tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };

                let conn_state = Arc::clone(&task_state);
                tokio::spawn(async move {
                    let Ok(mut ws_stream) = tokio_tungstenite::accept_async(stream).await else {
                        return;
                    };

                    while let Some(Ok(msg)) = ws_stream.next().await {
                        if !msg.is_text() {
                            continue;
                        }

                        let text = msg.to_text().unwrap_or_default();
                        let Ok(req_val) = serde_json::from_str::<Value>(text) else {
                            continue;
                        };

                        if let Some(resp_val) = process_tizen_request(&req_val, &conn_state) {
                            let resp_text = serde_json::to_string(&resp_val).unwrap_or_default();
                            if ws_stream
                                .send(Message::Text(resp_text.into()))
                                .await
                                .is_err()
                            {
                                break;
                            }
                        }
                    }
                });
            }
        });

        Ok(Self { addr, state, task })
    }

    /// Returns the server's bound address.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Returns the server's port.
    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    /// Access the state snapshot.
    pub fn state(&self) -> TizenDeviceState {
        self.state.lock().unwrap().clone()
    }

    /// Shuts down the server.
    pub fn shutdown(self) {
        self.task.abort();
    }
}

impl Drop for MockTizenServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn process_tizen_request(req: &Value, state: &Arc<Mutex<TizenDeviceState>>) -> Option<Value> {
    let method = req.get("method")?.as_str()?;
    let mut lock = state.lock().unwrap();

    match method {
        "ms.channel.connect" => Some(json!({
            "event": "ms.channel.connect",
            "data": {
                "token": lock.token,
                "name": lock.name
            }
        })),
        "ms.remote.control" => {
            if let Some(key) = req
                .get("params")
                .and_then(|p| p.get("DataOfCmd"))
                .and_then(Value::as_str)
            {
                lock.last_key = Some(key.to_string());
            }
            None
        }
        "ms.channel.emit" => {
            if let Some(app_id) = req
                .get("params")
                .and_then(|p| p.get("data"))
                .and_then(|d| d.get("appId"))
                .and_then(Value::as_str)
            {
                lock.last_launched_app = Some(app_id.to_string());
            }
            None
        }
        _ => None,
    }
}
