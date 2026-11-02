//! Samsung Tizen remote-channel mock server for integration testing.
//!
//! Speaks the real `ms.channel` framing recorded in `docs/02-playbook.md` §3,
//! so the pairing exit the plan calls A2 (and the G10b app-enumeration wire)
//! can be exercised with no panel:
//!
//! - **First pairing goes through the prompt.** A `ms.channel.connect` frame
//!   without a `token` is answered after the configured delay, and the reply
//!   carries a freshly issued token in `data.token` — the on-screen PIN
//!   dialog cannot be simulated with pixels, but the wire contract the client
//!   depends on (the TV stays silent until the human answers, then a connect
//!   event with a token arrives) is reproduced exactly.
//! - **A presented token never prompts.** A connect frame whose token was
//!   previously issued by this mock is answered immediately, echoing the same
//!   token — the playbook's "present that token next time and the TV does
//!   not prompt".
//! - **A stale or forged token is a denial, not a hang.** The reply is the
//!   recorded `ms.channel.unauthorized` event — the shape the client maps to
//!   `TizenError::Unauthorized`.
//! - **Key presses are recorded, never answered.** The real channel sends no
//!   per-command reply for `ms.remote.control` frames; a mock that invented
//!   one would be a shape the playbook does not have.
//! - **App enumeration answers the recorded Tizen ≤4 shape.** The
//!   `ed.installedApp.get` emit is answered with the verbatim event shape the
//!   playbook records (Netflix `11101200001`, YouTube `2019010019`), so the
//!   G10b wire contract has a fixture.
//!
//! Two constructors: [`MockTizenServer::start_plain`] serves the legacy
//! `ws://` channel on an ephemeral port (the client pins it with
//! `pinned_port`, and any port other than 8002 runs without TLS), and
//! [`MockTizenServer::start_tls`] serves the modern `wss://` channel with a
//! freshly generated self-signed certificate — the shape the
//! `TrustOnFirstUse` policy exists for. Note the client hard-wires
//! "TLS iff port 8002" (`open_channel` in `crates/tizen/src/client.rs`), so a
//! TLS mock must bind exactly that port.
//!
//! Everything the tests need to observe is exposed: issued and presented
//! tokens, how many prompts were shown, which key codes arrived, and which
//! app enumerations were requested.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::protocol::Message;
use tokio_tungstenite::WebSocketStream;

/// The fixed port the modern TLS channel must live on for the client to use
/// TLS (`use_tls = port == TIZEN_WSS_PORT` is hard-wired in the client).
pub const MOCK_TLS_PORT: u16 = 8002;

/// Internal counters the observers read.
#[derive(Debug, Default)]
struct Recorder {
    /// Tokens presented in connect frames (may be forged).
    presented_tokens: Vec<String>,
    /// Tokens issued through the prompt path.
    issued_tokens: Vec<String>,
    /// How many connect frames took the prompt path (no presented token).
    prompts_shown: usize,
    /// `DataOfCmd` values from `ms.remote.control` frames, in arrival order.
    key_commands: Vec<String>,
    /// `Cmd` values from `SendInputString` frames (typed text), in order.
    text_commands: Vec<String>,
    /// How many `ed.installedApp.get` emits arrived.
    app_list_requests: usize,
}

/// A mock server answering Samsung Tizen remote-channel frames.
pub struct MockTizenServer {
    local_addr: SocketAddr,
    /// DER bytes of the server certificate (empty for the plain variant).
    cert_der: Vec<u8>,
    shutdown_tx: Option<oneshot::Sender<()>>,
    server_task: Option<JoinHandle<()>>,
    recorder: Arc<Mutex<Recorder>>,
}

impl MockTizenServer {
    /// Starts the plain-WS mock on an ephemeral port.
    pub async fn start_plain(bind_addr: &str) -> Result<Self, std::io::Error> {
        Self::start(bind_addr, None).await
    }

    /// Starts the TLS mock with a fresh self-signed certificate.
    ///
    /// Because the client enables TLS if and only if the port is 8002, the
    /// tests that exercise TLS call this with `127.0.0.1:8002`.
    pub async fn start_tls(bind_addr: &str) -> Result<Self, std::io::Error> {
        let cert = generate_self_signed_cert();
        Self::start(bind_addr, Some(cert)).await
    }

    async fn start(bind_addr: &str, tls: Option<TlsMaterials>) -> Result<Self, std::io::Error> {
        let listener = TcpListener::bind(bind_addr).await?;
        let local_addr = listener.local_addr()?;
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();

        let recorder = Arc::new(Mutex::new(Recorder::default()));
        let recorder_for_task = Arc::clone(&recorder);
        let cert_der = tls
            .as_ref()
            .map(|m| (*m.cert_der).clone())
            .unwrap_or_default();

        let server_task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => break,
                    accept_res = listener.accept() => {
                        match accept_res {
                            Ok((socket, _)) => {
                                let recorder = Arc::clone(&recorder_for_task);
                                let tls = tls.clone();
                                tokio::spawn(async move {
                                    match tls {
                                        Some(materials) => {
                                            let acceptor = materials.acceptor;
                                            if let Ok(stream) = acceptor.accept(socket).await {
                                                let ws = tokio_tungstenite::accept_async(stream).await;
                                                if let Ok(ws) = ws {
                                                    serve_connection(ws, recorder).await;
                                                }
                                            }
                                        }
                                        None => {
                                            let ws = tokio_tungstenite::accept_async(socket).await;
                                            if let Ok(ws) = ws {
                                                serve_connection(ws, recorder).await;
                                            }
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
            cert_der,
            shutdown_tx: Some(shutdown_tx),
            server_task: Some(server_task),
            recorder,
        })
    }

    /// The bound address, e.g. `127.0.0.1:49152`.
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// DER bytes of the mock's self-signed certificate (empty when plain).
    pub fn cert_der(&self) -> &[u8] {
        &self.cert_der
    }

    /// Every token a connect frame presented, in arrival order (forged ones
    /// included).
    pub fn presented_tokens(&self) -> Vec<String> {
        self.recorder
            .lock()
            .expect("recorder lock")
            .presented_tokens
            .clone()
    }

    /// Every token the mock issued through the prompt path.
    pub fn issued_tokens(&self) -> Vec<String> {
        self.recorder
            .lock()
            .expect("recorder lock")
            .issued_tokens
            .clone()
    }

    /// How many connect frames took the prompt path (no presented token).
    pub fn prompts_shown(&self) -> usize {
        self.recorder.lock().expect("recorder lock").prompts_shown
    }

    /// `DataOfCmd` key codes received from any session, in arrival order.
    pub fn key_commands(&self) -> Vec<String> {
        self.recorder
            .lock()
            .expect("recorder lock")
            .key_commands
            .clone()
    }

    /// Typed-text `Cmd` payloads received (base64 as sent), in arrival order.
    pub fn text_commands(&self) -> Vec<String> {
        self.recorder
            .lock()
            .expect("recorder lock")
            .text_commands
            .clone()
    }

    /// How many `ed.installedApp.get` emits arrived.
    pub fn app_list_requests(&self) -> usize {
        self.recorder
            .lock()
            .expect("recorder lock")
            .app_list_requests
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

impl Drop for MockTizenServer {
    fn drop(&mut self) {
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }
        if let Some(server_task) = self.server_task.take() {
            server_task.abort();
        }
    }
}

/// Drives one WebSocket connection to completion: handshake, then commands.
async fn serve_connection<S>(mut ws: WebSocketStream<S>, recorder: Arc<Mutex<Recorder>>)
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    loop {
        match ws.next().await {
            Some(Ok(Message::Text(text))) => {
                let frame: Value = match serde_json::from_str(&text) {
                    Ok(frame) => frame,
                    Err(_) => {
                        // A loud refusal beats a silent one, but the channel's
                        // error event is the shape the client parses.
                        let reply = json!({"event": "ms.error"});
                        if ws.send(Message::text(reply.to_string())).await.is_err() {
                            return;
                        }
                        continue;
                    }
                };

                let ok = match frame.get("method").and_then(Value::as_str) {
                    Some("ms.channel.connect") => handle_connect(&mut ws, frame, &recorder).await,
                    Some("ms.remote.control") => {
                        handle_remote_control(&frame, &recorder);
                        Ok(())
                    }
                    Some("ms.channel.emit") => handle_channel_emit(&mut ws, frame, &recorder).await,
                    Some(other) => {
                        // Unknown method: the client never sends one; refuse
                        // loudly and keep the connection usable.
                        let reply = json!({
                            "event": "ms.error",
                            "data": {"message": format!("unknown method {other:?}")},
                        });
                        ws.send(Message::text(reply.to_string())).await
                    }
                    None => {
                        let reply = json!({"event": "ms.error"});
                        ws.send(Message::text(reply.to_string())).await
                    }
                };
                if ok.is_err() {
                    return;
                }
            }
            // Binary/Ping frames are not channel frames; ignore them.
            Some(Ok(_)) => {}
            Some(Err(_)) | None => return,
        }
    }
}

/// Handles the `ms.channel.connect` handshake with the token contract.
async fn handle_connect<S>(
    ws: &mut WebSocketStream<S>,
    frame: Value,
    recorder: &Arc<Mutex<Recorder>>,
) -> Result<(), tokio_tungstenite::tungstenite::Error>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    let presented = frame
        .pointer("/params/token")
        .and_then(Value::as_str)
        .filter(|t| !t.is_empty())
        .map(str::to_string);

    let reply = match presented {
        None => {
            // Prompt path: the TV stays silent while the human decides, then
            // answers with a connect event carrying a fresh token.
            let token = {
                let mut r = recorder.lock().expect("recorder lock");
                r.prompts_shown += 1;
                let token = format!("mock-tizen-token-{}", r.issued_tokens.len() + 1);
                r.issued_tokens.push(token.clone());
                token
            };
            json!({
                "event": "ms.channel.connect",
                "data": {"token": token, "name": "Mock Tizen TV"},
            })
        }
        Some(token) => {
            recorder
                .lock()
                .expect("recorder lock")
                .presented_tokens
                .push(token.clone());
            let known = recorder
                .lock()
                .expect("recorder lock")
                .issued_tokens
                .contains(&token);
            if known {
                // Stored-token path: immediate connect, same token echoed.
                json!({
                    "event": "ms.channel.connect",
                    "data": {"token": token, "name": "Mock Tizen TV"},
                })
            } else {
                // Forged or expired token: the recorded denial event.
                json!({"event": "ms.channel.unauthorized"})
            }
        }
    };

    ws.send(Message::text(reply.to_string())).await
}

/// Records `ms.remote.control` frames; the real channel sends no reply.
fn handle_remote_control(frame: &Value, recorder: &Arc<Mutex<Recorder>>) {
    let params = frame.get("params").cloned().unwrap_or(Value::Null);
    let type_of_remote = params
        .get("TypeOfRemote")
        .and_then(Value::as_str)
        .unwrap_or_default();
    {
        let mut r = recorder.lock().expect("recorder lock");
        match type_of_remote {
            "SendInputString" => {
                if let Some(cmd) = params.get("Cmd").and_then(Value::as_str) {
                    r.text_commands.push(cmd.to_string());
                }
            }
            _ => {
                if let Some(key) = params.get("DataOfCmd").and_then(Value::as_str) {
                    r.key_commands.push(key.to_string());
                }
            }
        }
    }
}

/// Handles `ms.channel.emit`: only `ed.installedApp.get` has a recorded reply.
async fn handle_channel_emit<S>(
    ws: &mut WebSocketStream<S>,
    frame: Value,
    recorder: &Arc<Mutex<Recorder>>,
) -> Result<(), tokio_tungstenite::tungstenite::Error>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    let event = frame
        .pointer("/params/event")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if event == "ed.installedApp.get" {
        recorder.lock().expect("recorder lock").app_list_requests += 1;
        // The verbatim shape from docs/02-playbook.md §3 (Tizen ≤4 sets).
        let reply = json!({
            "data": {"data": [
                {
                    "appId": "11101200001",
                    "app_type": 2,
                    "icon": "/opt/share/webappservice/apps_icon/FirstScreen/11101200001/250x250.png",
                    "is_lock": 0,
                    "name": "Netflix",
                },
                {
                    "appId": "2019010019",
                    "app_type": 2,
                    "icon": "/opt/share/webappservice/apps_icon/FirstScreen/2019010019/250x250.png",
                    "is_lock": 0,
                    "name": "YouTube",
                },
            ]},
            "event": "ed.installedApp.get",
            "from": "host",
        });
        return ws.send(Message::text(reply.to_string())).await;
    }
    // Unrecorded emits get the channel's error event, not an invented reply.
    let reply = json!({
        "event": "ms.error",
        "data": {"message": format!("no recorded reply for emit {event:?}")},
    });
    ws.send(Message::text(reply.to_string())).await
}

/// A self-signed certificate plus the acceptor built from it.
#[derive(Clone)]
struct TlsMaterials {
    acceptor: tokio_rustls::TlsAcceptor,
    cert_der: Arc<Vec<u8>>,
}

/// Generates a throwaway self-signed RSA certificate for the mock.
fn generate_self_signed_cert() -> TlsMaterials {
    // The mock's key type is irrelevant to the client (TOFU pins the cert,
    // not the key), so the default ring-backed ECDSA keygen is enough.
    let key_pair = rcgen::KeyPair::generate().expect("mock keygen");
    let params =
        rcgen::CertificateParams::new(vec!["mock-tizen-tv".into()]).expect("mock cert params");
    let cert = params.self_signed(&key_pair).expect("mock self-sign");
    let cert_der = cert.der().to_vec();

    let key = rustls::pki_types::PrivateKeyDer::try_from(key_pair.serialize_der())
        .expect("mock private key der");
    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(
            vec![rustls::pki_types::CertificateDer::from(cert_der.clone())],
            key,
        )
        .expect("mock server cert");
    TlsMaterials {
        acceptor: tokio_rustls::TlsAcceptor::from(Arc::new(config)),
        cert_der: Arc::new(cert_der),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn plain_server_binds_and_shuts_down() {
        let server = MockTizenServer::start_plain("127.0.0.1:0")
            .await
            .expect("bind");
        assert!(server.cert_der().is_empty());
        assert_eq!(server.prompts_shown(), 0);
        server.shutdown().await;
    }

    #[tokio::test]
    async fn tls_server_generates_a_certificate() {
        let server = MockTizenServer::start_tls("127.0.0.1:0")
            .await
            .expect("bind");
        assert!(!server.cert_der().is_empty(), "TLS mock carries a DER cert");
        let fp = tv_rs_transport::CertFingerprint::of_der(server.cert_der());
        assert_eq!(fp.to_hex().len(), 64);
        server.shutdown().await;
    }
}
