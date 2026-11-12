//! LG webOS SSAP hardware mock server for integration testing.
//!
//! Speaks the real SSAP framing from the client side's recorded shapes
//! (the `tv_rs_drivers::webos` crate parses `register` / `registered` / `error` frames and the
//! `response` envelope its `SsapResponse` struct captures), so the pairing
//! exit the plan calls A2 can be exercised with no panel:
//!
//! - **First pairing goes through the prompt.** A `register` frame without a
//!   `client-key` is answered after the configured decision: acceptance
//!   returns a `registered` frame carrying the issued `client-key`; denial
//!   returns an `error` frame. The on-screen "Allow?" cannot be simulated
//!   with pixels — but the wire contract the client depends on (the TV stays
//!   silent until the human answers, and the answer is one of those two
//!   frames) is reproduced exactly.
//! - **A presented `client-key` never prompts.** The mock answers a register
//!   frame that carries a non-empty `client-key` immediately, echoing the
//!   same key back — the playbook's "send it in later `register` payloads
//!   and the prompt never appears again".
//! - **Commands echo the request id** and answer with the recorded SSAP
//!   envelope (`{"returnValue": true}`), documented for the IME service in
//!   `docs/02-playbook.md` §4. The mock does not model state behind any
//!   command: a setVolume that claims to apply is out of scope until the
//!   playbook records the reply shapes, and a mock inventing unrecorded
//!   shapes is exactly what the plan forbids.
//!
//! Everything the tests need to observe is exposed: issued and presented
//! keys, how many prompts were shown, and which command URIs arrived.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::protocol::Message;
use tokio_tungstenite::WebSocketStream;

/// What the fake television does with a first-time pairing request.
#[derive(Debug, Clone)]
pub enum PromptAcceptance {
    /// The viewer answers "Allow": pairing succeeds after `delay` (the time
    /// the prompt stays on screen), and the mock issues `client_key`.
    Accept {
        /// How long the simulated prompt waits before acceptance.
        delay: Duration,
        /// The `client-key` the television issues on acceptance.
        client_key: String,
    },
    /// The viewer dismisses the prompt: pairing fails with an `error` frame.
    ///
    /// The code and message are mock-chosen; the frame *shape* is what the
    /// client's recorded types parse.
    Deny,
}

impl PromptAcceptance {
    /// An immediate acceptance issuing the default mock key.
    #[must_use]
    pub fn accept_now() -> Self {
        Self::Accept {
            delay: Duration::from_millis(50),
            client_key: "mock-client-key".to_string(),
        }
    }
}

/// Internal counters the observers read.
#[derive(Debug, Default)]
struct Recorder {
    /// `client-key` values presented in register frames.
    presented_keys: Vec<String>,
    /// `client-key` values issued through acceptance.
    issued_keys: Vec<String>,
    /// How many register frames went through the prompt path.
    prompts_shown: usize,
    /// Command URIs that arrived (one entry per request frame).
    commands: Vec<String>,
}

/// A WebSocket mock server answering LG webOS SSAP frames.
pub struct MockWebOsServer {
    local_addr: SocketAddr,
    shutdown_tx: Option<oneshot::Sender<()>>,
    server_task: Option<JoinHandle<()>>,
    recorder: Arc<Mutex<Recorder>>,
}

impl MockWebOsServer {
    /// Starts the mock with an immediate the-prompt-gets-accepted policy.
    pub async fn start(bind_addr: &str) -> Result<Self, std::io::Error> {
        Self::start_with_acceptance(bind_addr, PromptAcceptance::accept_now()).await
    }

    /// Starts the mock with an explicit prompt policy.
    pub async fn start_with_acceptance(
        bind_addr: &str,
        acceptance: PromptAcceptance,
    ) -> Result<Self, std::io::Error> {
        let listener = TcpListener::bind(bind_addr).await?;
        let local_addr = listener.local_addr()?;
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();

        let recorder = Arc::new(Mutex::new(Recorder::default()));
        let recorder_for_task = Arc::clone(&recorder);
        let acceptance = Arc::new(acceptance);

        let server_task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => break,
                    accept_res = listener.accept() => {
                        match accept_res {
                            Ok((socket, _)) => {
                                let recorder = Arc::clone(&recorder_for_task);
                                let acceptance = Arc::clone(&acceptance);
                                tokio::spawn(async move {
                                    let ws = tokio_tungstenite::accept_async(socket).await;
                                    if let Ok(ws) = ws {
                                        serve_connection(ws, recorder, acceptance).await;
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
            recorder,
        })
    }

    /// The bound address, e.g. `127.0.0.1:49152`.
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Every `client-key` a register frame presented, in arrival order.
    pub fn presented_client_keys(&self) -> Vec<String> {
        self.recorder
            .lock()
            .expect("recorder lock")
            .presented_keys
            .clone()
    }

    /// Every `client-key` the mock issued through an acceptance.
    pub fn issued_client_keys(&self) -> Vec<String> {
        self.recorder
            .lock()
            .expect("recorder lock")
            .issued_keys
            .clone()
    }

    /// How many register frames took the prompt path (no presented key).
    pub fn prompts_shown(&self) -> usize {
        self.recorder.lock().expect("recorder lock").prompts_shown
    }

    /// Command URIs received from any session, in arrival order.
    pub fn commands(&self) -> Vec<String> {
        self.recorder
            .lock()
            .expect("recorder lock")
            .commands
            .clone()
    }

    /// Stops the server, waiting for the accept loop to exit.
    pub async fn shutdown(mut self) {
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }
        if let Some(server_task) = self.server_task.take() {
            let _ = server_task.await;
        }
    }
}

impl Drop for MockWebOsServer {
    fn drop(&mut self) {
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }
        if let Some(server_task) = self.server_task.take() {
            server_task.abort();
        }
    }
}

/// Drives one WebSocket connection to completion: register, then commands.
///
/// A television that walks away mid-handshake is modelled by simply returning
/// when the client's socket closes — the same silence a powered-off panel
/// gives, which is the honest shape of "the TV is off".
async fn serve_connection<S>(
    mut ws: WebSocketStream<S>,
    recorder: Arc<Mutex<Recorder>>,
    acceptance: Arc<PromptAcceptance>,
) where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    loop {
        match ws.next().await {
            Some(Ok(Message::Text(text))) => {
                let frame: Value = match serde_json::from_str(&text) {
                    Ok(frame) => frame,
                    // Unparseable frames are answered the error way rather than
                    // dropping the connection: this is a test double, and a
                    // loud refusal beats a silent one.
                    Err(_) => {
                        let reply = error_frame(None, -2, "not a parseable SSAP frame");
                        if ws.send(Message::text(reply.to_string())).await.is_err() {
                            return;
                        }
                        continue;
                    }
                };

                let ok = match frame.get("type").and_then(Value::as_str) {
                    Some("register") => {
                        handle_register(&mut ws, frame, &recorder, &acceptance).await
                    }
                    Some("request") => handle_request(&mut ws, frame, &recorder).await,
                    Some(kind) => {
                        // An unknown frame type is a client bug; refuse loudly
                        // but keep the connection usable.
                        let reply = error_frame(
                            frame.get("id").and_then(Value::as_i64),
                            -3,
                            &format!("unknown SSAP frame type {kind:?}"),
                        );
                        ws.send(Message::text(reply.to_string())).await
                    }
                    None => {
                        let reply = error_frame(None, -3, "SSAP frame without a type");
                        ws.send(Message::text(reply.to_string())).await
                    }
                };
                if ok.is_err() {
                    return;
                }
            }
            // Binary/Ping frames are not SSAP; ignore them like the client does.
            Some(Ok(_)) => {}
            Some(Err(_)) | None => return,
        }
    }
}

/// Builds an `error` frame of the recorded shape, with an optional id echo.
fn error_frame(id: Option<i64>, code: i64, message: &str) -> Value {
    json!({
        "type": "error",
        "id": id,
        "error": { "code": code, "message": message },
    })
}

/// Answers one `register` frame: prompt path or silent stored-key path.
async fn handle_register<S>(
    ws: &mut WebSocketStream<S>,
    frame: Value,
    recorder: &Arc<Mutex<Recorder>>,
    acceptance: &PromptAcceptance,
) -> Result<(), tokio_tungstenite::tungstenite::Error>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    // The client always sends `id` as a JSON string (e.g. `"1"`). Keep the
    // reply's id in the same shape so the client's correlation match holds.
    let id = frame.get("id").and_then(Value::as_str).map(String::from);

    let presented_key = frame
        .pointer("/payload/client-key")
        .and_then(Value::as_str)
        .filter(|key| !key.is_empty())
        .map(str::to_string);

    let reply = match presented_key {
        Some(presented) => {
            // The playbook's contract: a presented key pairs silently — no
            // prompt, the same key confirmed back.
            recorder
                .lock()
                .expect("recorder lock")
                .presented_keys
                .push(presented.clone());
            json!({
                "type": "registered",
                "id": id,
                "payload": { "client-key": presented },
            })
        }
        None => match acceptance {
            PromptAcceptance::Accept { delay, client_key } => {
                // The "Allow?" is on screen: the television says nothing
                // until it is answered. The delay is that silence, made
                // observable.
                tokio::time::sleep(*delay).await;
                {
                    let mut items = recorder.lock().expect("recorder lock");
                    items.prompts_shown += 1;
                    items.issued_keys.push(client_key.clone());
                }
                json!({
                    "type": "registered",
                    "id": id,
                    "payload": { "client-key": client_key },
                })
            }
            PromptAcceptance::Deny => {
                recorder.lock().expect("recorder lock").prompts_shown += 1;
                json!({
                    "type": "error",
                    "id": id,
                    "error": {
                        "code": -1,
                        "message": "pairing denied by the mock",
                    },
                })
            }
        },
    };

    ws.send(Message::text(reply.to_string())).await?;
    Ok(())
}

/// Answers one command `request` frame with the recorded reply envelope.
async fn handle_request<S>(
    ws: &mut WebSocketStream<S>,
    frame: Value,
    recorder: &Arc<Mutex<Recorder>>,
) -> Result<(), tokio_tungstenite::tungstenite::Error>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    // The client always sends `id` as a JSON string (e.g. `"2"`). Keep the
    // reply's id in the same shape so the client's correlation match holds.
    let id = frame.get("id").and_then(Value::as_str).map(String::from);
    let uri = frame
        .get("uri")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    recorder
        .lock()
        .expect("recorder lock")
        .commands
        .push(uri.clone());

    // The uniform reply envelope the client matches on; the mock models no
    // state behind any command until the playbook records those shapes.
    let reply = json!({
        "type": "response",
        "id": id,
        "payload": { "returnValue": true },
    });

    ws.send(Message::text(reply.to_string())).await?;

    Ok(())
}
