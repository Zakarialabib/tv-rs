//! Mock LG webOS SSAP WebSocket protocol server.

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;

/// Simulated state for a webOS TV.
#[derive(Debug, Clone)]
pub struct WebosDeviceState {
    /// Volume level (0..100).
    pub volume: u8,
    /// Mute state.
    pub mute: bool,
    /// Whether the panel has been turned off via SSAP.
    pub power_off: bool,
    /// Currently foreground application ID.
    pub foreground_app: Option<String>,
    /// Issued client key for registration.
    pub client_key: String,
}

impl Default for WebosDeviceState {
    fn default() -> Self {
        Self {
            volume: 15,
            mute: false,
            power_off: false,
            foreground_app: Some("com.webos.app.home".to_string()),
            client_key: "sim-webos-client-key-12345".to_string(),
        }
    }
}

/// Mock webOS SSAP Server.
pub struct MockWebosServer {
    addr: SocketAddr,
    state: Arc<Mutex<WebosDeviceState>>,
    task: JoinHandle<()>,
}

impl MockWebosServer {
    /// Starts a mock webOS SSAP server on an ephemeral local port (`127.0.0.1:0`).
    pub async fn start() -> Result<Self, std::io::Error> {
        Self::start_on("127.0.0.1:0").await
    }

    /// Starts a mock webOS SSAP server on `bind_addr`.
    pub async fn start_on(bind_addr: &str) -> Result<Self, std::io::Error> {
        let listener = TcpListener::bind(bind_addr).await?;
        let addr = listener.local_addr()?;
        let state = Arc::new(Mutex::new(WebosDeviceState::default()));

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

                        if let Some(resp_val) = process_ssap_request(&req_val, &conn_state) {
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
    pub fn state(&self) -> WebosDeviceState {
        self.state.lock().unwrap().clone()
    }

    /// Shuts down the server.
    pub fn shutdown(self) {
        self.task.abort();
    }
}

impl Drop for MockWebosServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn process_ssap_request(req: &Value, state: &Arc<Mutex<WebosDeviceState>>) -> Option<Value> {
    let req_type = req.get("type")?.as_str()?;
    let req_id = req.get("id").and_then(Value::as_str).unwrap_or("1");

    if req_type == "register" {
        let lock = state.lock().unwrap();
        return Some(json!({
            "id": req_id,
            "type": "registered",
            "payload": {
                "client-key": lock.client_key
            }
        }));
    }

    if req_type == "request" {
        let uri = req.get("uri")?.as_str()?;
        let mut lock = state.lock().unwrap();

        let payload = match uri {
            "ssap://audio/getVolume" => json!({
                "returnValue": true,
                "volume": lock.volume,
                "muted": lock.mute
            }),
            "ssap://audio/setVolume" => {
                if let Some(vol) = req
                    .get("payload")
                    .and_then(|p| p.get("volume"))
                    .and_then(Value::as_u64)
                {
                    lock.volume = vol as u8;
                }
                json!({ "returnValue": true })
            }
            "ssap://audio/getMute" => json!({
                "returnValue": true,
                "mute": lock.mute
            }),
            "ssap://audio/setMute" => {
                if let Some(m) = req
                    .get("payload")
                    .and_then(|p| p.get("mute"))
                    .and_then(Value::as_bool)
                {
                    lock.mute = m;
                }
                json!({ "returnValue": true })
            }
            "ssap://system/turnOff" => {
                lock.power_off = true;
                json!({ "returnValue": true })
            }
            "ssap://system.launcher/launch" => {
                if let Some(app_id) = req
                    .get("payload")
                    .and_then(|p| p.get("id"))
                    .and_then(Value::as_str)
                {
                    lock.foreground_app = Some(app_id.to_string());
                }
                json!({ "returnValue": true })
            }
            "ssap://system.launcher/getForegroundAppInfo" => json!({
                "returnValue": true,
                "appId": lock.foreground_app
            }),
            "ssap://com.webos.applicationManager/listApps" => json!({
                "returnValue": true,
                "apps": [
                    { "id": "com.webos.app.home", "title": "Home" },
                    { "id": "youtube.leanback.v4", "title": "YouTube" },
                    { "id": "netflix", "title": "Netflix" }
                ]
            }),
            _ => json!({ "returnValue": true }),
        };

        return Some(json!({
            "id": req_id,
            "type": "response",
            "payload": payload
        }));
    }

    None
}
