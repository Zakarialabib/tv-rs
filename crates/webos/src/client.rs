//! Pure helpers, the SSAP client configuration and the live WebSocket session.

use std::path::Path;

use futures_util::SinkExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio_tungstenite::connect_async_tls_with_config;
use tokio_tungstenite::tungstenite::Message;
use tv_rs_transport::{read_until, ReadOutcome, TlsPolicy, WsStream};

use crate::error::{Result, WebosError};
use crate::payload::{
    SsapManifest, SsapRegisterPayload, SsapRegisterRequest, SsapRegisterResponse, SsapRequest,
    SsapResponse,
};
use crate::types::{SsapUri, WEBOS_PAIRING_TYPE_PROMPT, WEBOS_WSS_PORT, WEBOS_WS_PORT};

/// Correlation id used for the registration frame.
const REGISTER_REQUEST_ID: &str = "1";

/// Default number of frames read while waiting for a matching response.
const DEFAULT_FRAME_BUDGET: usize = 32;

/// How long one connection attempt may take before it counts as "not listening".
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Builds an SSAP `register` request with the [`WEBOS_PAIRING_TYPE_PROMPT`] pairing type.
#[must_use]
pub fn build_register_request(id: &str, manifest: SsapManifest) -> SsapRegisterRequest {
    SsapRegisterRequest {
        id: id.to_string(),
        kind: "register".to_string(),
        payload: SsapRegisterPayload {
            pairing_type: WEBOS_PAIRING_TYPE_PROMPT.to_string(),
            manifest,
        },
    }
}

/// Parses any SSAP response frame (command `response` or `error`).
pub fn parse_ssap_response(raw: &str) -> Result<SsapResponse> {
    Ok(serde_json::from_str(raw)?)
}

/// Parses an SSAP registration response frame.
pub fn parse_register_response(raw: &str) -> Result<SsapRegisterResponse> {
    Ok(serde_json::from_str(raw)?)
}

/// Returns the non-empty `payload.client-key` from a registration response.
#[must_use]
pub fn extract_client_key(response: &SsapRegisterResponse) -> Option<String> {
    response
        .payload
        .as_ref()
        .and_then(|payload| payload.client_key.clone())
        .filter(|client_key| !client_key.is_empty())
}

/// Builds an SSAP command request with `type` set to `"request"`.
#[must_use]
pub fn build_request(id: &str, uri: SsapUri, payload: Option<Value>) -> SsapRequest {
    SsapRequest {
        id: id.to_string(),
        kind: "request".to_string(),
        uri: uri.as_str().to_string(),
        payload,
    }
}

/// The on-disk representation of a persisted `client-key`.
#[derive(Debug, Serialize, Deserialize)]
struct SsapClientKeyFile {
    #[serde(rename = "client-key")]
    client_key: String,
}

/// Persists a `client-key` as a tiny JSON document `{"client-key":"..."}`.
pub async fn save_client_key(path: impl AsRef<Path>, client_key: &str) -> Result<()> {
    let body = serde_json::to_string(&SsapClientKeyFile {
        client_key: client_key.to_string(),
    })?;
    tokio::fs::write(path, body).await?;
    Ok(())
}

/// Loads a persisted `client-key`, returning `Ok(None)` when the file is absent.
pub async fn load_client_key(path: impl AsRef<Path>) -> Result<Option<String>> {
    match tokio::fs::read_to_string(path).await {
        Ok(raw) => Ok(Some(
            serde_json::from_str::<SsapClientKeyFile>(&raw)?.client_key,
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// Configuration for talking to a single LG webOS TV.
///
/// Use the `with_*` builder methods to change defaults, then call
/// [`WebosClient::connect`] (or [`WebosClient::pair`] on first use).
#[derive(Clone)]
pub struct WebosClient {
    host: String,
    port: u16,
    tls: TlsPolicy,
    client_key: Option<String>,
    manifest: SsapManifest,
    device_name: String,
    /// Which port actually answered, once one has. Shared across clones.
    negotiated: Arc<Mutex<Option<u16>>>,
    /// A port the caller knows is the only one this panel listens on.
    pinned: Option<u16>,
}

impl PartialEq for WebosClient {
    /// Compares configuration, not the negotiated port: two clients configured the
    /// same way are the same client, even if one has connected and the other has
    /// not yet.
    fn eq(&self, other: &Self) -> bool {
        self.host == other.host
            && self.port == other.port
            && self.tls == other.tls
            && self.client_key == other.client_key
            && self.manifest == other.manifest
            && self.device_name == other.device_name
    }
}

impl std::fmt::Debug for WebosClient {
    /// Renders the client without its `client-key`.
    ///
    /// The key is a bearer credential for the TV; a log that prints it hands over
    /// control of the screen.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebosClient")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("tls", &self.tls)
            .field(
                "client_key",
                &self.client_key.as_ref().map(|_| "<redacted>"),
            )
            .field("manifest", &self.manifest)
            .field("device_name", &self.device_name)
            .field("negotiated_port", &self.negotiated_port())
            .finish()
    }
}

impl WebosClient {
    /// Creates a client for `host` using the legacy port `3000` and strict TLS.
    #[must_use]
    pub fn new(host: impl Into<String>) -> Self {
        Self {
            host: host.into(),
            port: WEBOS_WS_PORT,
            tls: TlsPolicy::default(),
            client_key: None,
            manifest: SsapManifest::control(),
            device_name: "tv-rs".to_string(),
            negotiated: Arc::new(Mutex::new(None)),
            pinned: None,
        }
    }

    /// Overrides the WebSocket port.
    #[must_use]
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self.forget_port();
        self
    }

    /// Uses `port` and **only** `port`.
    ///
    /// For a panel whose generation is already known (the quirks table: webOS 23 and
    /// later drop the plaintext port), the other port is a guaranteed timeout, so it
    /// leaves the ladder rather than becoming a slow failure.
    #[must_use]
    pub fn pinned_port(mut self, port: u16) -> Self {
        self.port = port;
        self.pinned = Some(port);
        *self.negotiated.lock().expect("negotiated lock") = None;
        self
    }

    /// The port this client is pinned to, if any.
    #[must_use]
    pub fn pinned_port_number(&self) -> Option<u16> {
        self.pinned
    }

    /// Switches to the modern TLS port `3001`.
    #[must_use]
    pub fn with_wss(mut self) -> Self {
        self.port = WEBOS_WSS_PORT;
        self.forget_port();
        self
    }

    /// Overrides the TLS certificate policy for port `3001` connections.
    #[must_use]
    pub fn with_tls_policy(mut self, policy: TlsPolicy) -> Self {
        self.tls = policy;
        self
    }

    /// Supplies a previously persisted `client-key` so the TV does not prompt again.
    #[must_use]
    pub fn with_client_key(mut self, client_key: impl Into<String>) -> Self {
        self.client_key = Some(client_key.into());
        self
    }

    /// Overrides the registration manifest.
    #[must_use]
    pub fn with_manifest(mut self, manifest: SsapManifest) -> Self {
        self.manifest = manifest;
        self
    }

    /// Sets the device name shown in the TV's on-screen "Allow?" prompt.
    #[must_use]
    pub fn with_device_name(mut self, device_name: impl Into<String>) -> Self {
        self.device_name = device_name.into();
        self
    }

    /// Returns the configured host.
    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    /// Returns the configured WebSocket port.
    #[must_use]
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Returns the configured TLS policy.
    #[must_use]
    pub fn tls_policy(&self) -> TlsPolicy {
        self.tls.clone()
    }

    /// Returns the currently configured `client-key`, if any.
    #[must_use]
    pub fn client_key(&self) -> Option<&str> {
        self.client_key.as_deref()
    }

    /// Returns the configured device name.
    #[must_use]
    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    /// The ports to try, most likely first.
    ///
    /// Once a port has answered, only that port is tried: re-probing costs two dead
    /// connections per command on a TV that already told us which one it uses.
    fn port_candidates(&self) -> Vec<u16> {
        if let Some(port) = *self.negotiated.lock().expect("negotiated lock") {
            return vec![port];
        }
        if let Some(port) = self.pinned {
            return vec![port];
        }
        let other = if self.port == WEBOS_WSS_PORT {
            WEBOS_WS_PORT
        } else {
            WEBOS_WSS_PORT
        };
        vec![self.port, other]
    }

    fn remember_port(&self, port: u16) {
        *self.negotiated.lock().expect("negotiated lock") = Some(port);
    }

    fn forget_port(&self) {
        *self.negotiated.lock().expect("negotiated lock") = None;
    }

    /// The port that last answered, if any.
    #[must_use]
    pub fn negotiated_port(&self) -> Option<u16> {
        *self.negotiated.lock().expect("negotiated lock")
    }

    fn ws_url_for(&self, port: u16) -> Result<String> {
        let scheme = if port == WEBOS_WSS_PORT { "wss" } else { "ws" };
        let url = url::Url::parse(&format!("{scheme}://{}:{port}/", self.host))?;
        Ok(url.to_string())
    }

    /// Builds the `ws://` or `wss://` WebSocket URL for this client.
    ///
    /// webOS serves SSAP on the root path, so the URL always ends in `/`. The
    /// scheme is `wss` when the port is [`WEBOS_WSS_PORT`], otherwise `ws`.
    pub fn ws_url(&self) -> Result<String> {
        let scheme = if self.port == WEBOS_WSS_PORT {
            "wss"
        } else {
            "ws"
        };
        let url = url::Url::parse(&format!("{scheme}://{}:{}/", self.host, self.port))?;
        Ok(url.to_string())
    }

    /// Opens the WebSocket, registers with the TV and returns a live session.
    ///
    /// When a `client-key` is configured it is sent during registration so the TV
    /// does not prompt again; the key observed from the TV is exposed through
    /// [`WebosSession::client_key`].
    pub async fn connect(&self) -> Result<WebosSession> {
        let mut last_error: Option<WebosError> = None;

        for port in self.port_candidates() {
            let url = self.ws_url_for(port)?;
            let connector = tv_rs_transport::connector(self.tls.clone(), port == WEBOS_WSS_PORT)
                .map_err(|error| {
                    WebosError::Protocol(format!("failed to build TLS connector: {error}"))
                })?;

            let attempt = tokio::time::timeout(
                CONNECT_TIMEOUT,
                connect_async_tls_with_config(url.as_str(), None, false, Some(connector)),
            )
            .await;

            let stream = match attempt {
                Ok(Ok((stream, _response))) => {
                    self.remember_port(port);
                    stream
                }
                // Nothing answered here: the other generation is worth a try.
                Ok(Err(error)) if tv_rs_transport::should_try_next_port(&error) => {
                    last_error = Some(WebosError::Ws(error));
                    continue;
                }
                // A certificate or protocol answer is not a port problem.
                Ok(Err(error)) => return Err(WebosError::Ws(error)),
                Err(_elapsed) => {
                    last_error = Some(WebosError::Timeout);
                    continue;
                }
            };

            return self.register(stream).await;
        }

        Err(last_error.unwrap_or(WebosError::Timeout))
    }

    /// Sends the SSAP registration and waits for the `registered` reply.
    async fn register(&self, mut stream: WsStream) -> Result<WebosSession> {
        let mut register = serde_json::to_value(build_register_request(
            REGISTER_REQUEST_ID,
            self.manifest.clone(),
        ))?;
        if let Some(client_key) = self.client_key.as_ref() {
            if let Some(payload) = register.get_mut("payload").and_then(Value::as_object_mut) {
                payload.insert("client-key".to_string(), Value::String(client_key.clone()));
            }
        }
        stream
            .send(Message::text(serde_json::to_string(&register)?))
            .await?;

        let mut client_key = self.client_key.clone();
        let mut budget = DEFAULT_FRAME_BUDGET;
        let outcome = read_until(&mut stream, &mut budget, |message| {
            if !message.is_text() {
                return Ok(None);
            }

            let response = parse_register_response(&message.into_text()?)?;
            match response.kind.as_deref() {
                Some("registered") => {
                    if let Some(issued) = extract_client_key(&response) {
                        client_key = Some(issued);
                    }
                    Ok(Some(()))
                }
                Some("error") => {
                    let (code, message) = response
                        .error
                        .map(|error| (error.code, error.message))
                        .unwrap_or((0, "unknown registration error".to_string()));
                    Err(WebosError::RegistrationFailed {
                        detail: format!("ssap registration failed ({code}): {message}"),
                        // `client_key` at this point is what the session presented.
                        presented_key: self.client_key.is_some(),
                    })
                }
                _ => Ok(None),
            }
        })
        .await?;

        match outcome {
            ReadOutcome::Ready(()) => {}
            ReadOutcome::Closed => {
                return Err(WebosError::Protocol(
                    "websocket closed during registration".to_string(),
                ))
            }
            ReadOutcome::BudgetExhausted => return Err(WebosError::Timeout),
            ReadOutcome::Failed(error) => return Err(error.into()),
        }

        Ok(WebosSession {
            stream,
            next_id: 1,
            client_key,
            frame_budget: DEFAULT_FRAME_BUDGET,
        })
    }

    /// Connects and returns just the pairing outcome.
    pub async fn pair(&self) -> Result<WebosPairing> {
        let session = self.connect().await?;
        Ok(WebosPairing {
            client_key: session.client_key().map(str::to_string),
            device_name: self.device_name.clone(),
        })
    }
}

/// The result of a successful pairing handshake.
#[derive(Debug, Clone, PartialEq)]
pub struct WebosPairing {
    /// The `client-key` issued by the TV, to be persisted by the caller.
    pub client_key: Option<String>,
    /// The device name shown in the TV's on-screen prompt.
    pub device_name: String,
}

/// A registered, live SSAP WebSocket session.
pub struct WebosSession {
    stream: WsStream,
    next_id: u64,
    client_key: Option<String>,
    frame_budget: usize,
}

impl WebosSession {
    /// Returns the `client-key` associated with this session, if any.
    #[must_use]
    pub fn client_key(&self) -> Option<&str> {
        self.client_key.as_deref()
    }

    /// Sends `uri` with a fresh request id and returns the matching `payload`.
    ///
    /// Non-text frames are skipped and frames whose `id` does not match are
    /// ignored, so unsolicited TV messages do not desynchronise the session. An
    /// `error` frame is converted into [`WebosError::Ssap`]. When the frame budget
    /// is exhausted before a match arrives, [`WebosError::Timeout`] is returned.
    pub async fn request(&mut self, uri: SsapUri, payload: Option<Value>) -> Result<Value> {
        self.next_id = self.next_id.saturating_add(1);
        let id = self.next_id.to_string();
        let raw = serde_json::to_string(&build_request(&id, uri, payload))?;
        self.send_raw(&raw).await?;

        let mut budget = self.frame_budget;
        let outcome = read_until(&mut self.stream, &mut budget, |message| {
            if !message.is_text() {
                return Ok(None);
            }

            let response = parse_ssap_response(&message.into_text()?)?;
            match response.id.as_deref() {
                Some(response_id) if response_id == id => {}
                _ => return Ok(None),
            }

            if let Some(error) = response.error {
                return Err(WebosError::Ssap {
                    code: error.code,
                    message: error.message,
                });
            }
            Ok(Some(response.payload.unwrap_or(Value::Null)))
        })
        .await?;

        match outcome {
            ReadOutcome::Ready(payload) => Ok(payload),
            ReadOutcome::Closed => Err(WebosError::Protocol(
                "websocket closed before a response arrived".to_string(),
            )),
            ReadOutcome::BudgetExhausted => Err(WebosError::Timeout),
            ReadOutcome::Failed(error) => Err(error.into()),
        }
    }

    /// Sets the output volume, clamped to `0..=100`.
    pub async fn set_volume(&mut self, volume: u8) -> Result<()> {
        let volume = volume.min(100);
        self.request(
            SsapUri::AudioSetVolume,
            Some(serde_json::json!({ "volume": volume })),
        )
        .await?;
        Ok(())
    }

    /// Opens a URL in the TV's built-in browser.
    ///
    /// The URL must be reachable *from the television*, which is not the same
    /// thing as reachable from the machine calling this: a `localhost` address
    /// here points at the TV itself.
    pub async fn open_url(&mut self, url: &str) -> Result<()> {
        self.request(
            SsapUri::SystemLauncherOpen,
            Some(serde_json::json!({ "target": url })),
        )
        .await?;
        Ok(())
    }

    /// Reads the current output volume.
    pub async fn get_volume(&mut self) -> Result<u8> {
        let payload = self.request(SsapUri::AudioGetVolume, None).await?;
        payload
            .get("volume")
            .and_then(Value::as_u64)
            .map(|volume| volume.min(100) as u8)
            .ok_or_else(|| {
                WebosError::Protocol("audio/getVolume response missing 'volume'".to_string())
            })
    }

    /// Enables or disables mute.
    pub async fn set_mute(&mut self, mute: bool) -> Result<()> {
        self.request(
            SsapUri::AudioSetMute,
            Some(serde_json::json!({ "mute": mute })),
        )
        .await?;
        Ok(())
    }

    /// Launches an installed application by id.
    pub async fn launch_app(&mut self, app_id: &str) -> Result<()> {
        self.request(
            SsapUri::SystemLauncherLaunch,
            Some(serde_json::json!({ "id": app_id })),
        )
        .await?;
        Ok(())
    }

    /// Turns the TV off.
    pub async fn power_off(&mut self) -> Result<()> {
        self.request(SsapUri::SystemTurnOff, None).await?;
        Ok(())
    }

    /// Returns the id of the foreground application, when the TV reports one.
    pub async fn foreground_app(&mut self) -> Result<Option<String>> {
        let payload = self
            .request(SsapUri::SystemLauncherForegroundApp, None)
            .await?;
        Ok(payload
            .get("appId")
            .and_then(Value::as_str)
            .map(str::to_string))
    }

    /// Sends a raw, pre-serialized frame over the socket.
    pub async fn send_raw(&mut self, raw: &str) -> Result<()> {
        self.stream.send(Message::text(raw)).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        build_register_request, build_request, extract_client_key, parse_register_response,
        parse_ssap_response, WebosClient,
    };
    use crate::payload::SsapManifest;
    use crate::types::SsapUri;
    use crate::TlsPolicy;
    use crate::{WEBOS_WSS_PORT, WEBOS_WS_PORT};

    #[test]
    fn a_pinned_port_is_the_only_candidate() {
        let client = WebosClient::new("192.168.1.20")
            .with_wss()
            .pinned_port(3001);
        assert_eq!(client.pinned_port_number(), Some(3001));
        assert_eq!(client.port_candidates(), vec![3001], "no fallback to 3000");

        let ladder = WebosClient::new("192.168.1.20").with_wss();
        assert_eq!(ladder.port_candidates(), vec![3001, 3000]);
    }

    #[test]
    fn build_register_request_serializes_expected_fields() {
        let raw = serde_json::to_string(&build_register_request("1", SsapManifest::control()))
            .expect("register request serializes");
        let value: serde_json::Value = serde_json::from_str(&raw).expect("valid json");

        assert_eq!(value["type"], "register");
        assert_eq!(value["payload"]["pairingType"], "PROMPT");
        assert_eq!(value["payload"]["manifest"]["manifestVersion"], 1);
        assert_eq!(
            value["payload"]["manifest"]["signed"],
            serde_json::Value::Null
        );
        let permissions = value["payload"]["manifest"]["permissions"]
            .as_array()
            .expect("permissions array");
        assert!(permissions.iter().any(|item| item == "CONTROL_POWER"));
    }

    #[test]
    fn parse_register_response_extracts_client_key() {
        let response = parse_register_response(
            r#"{"id":"1","type":"registered","payload":{"client-key":"abc123"}}"#,
        )
        .expect("valid register response");
        assert_eq!(extract_client_key(&response).as_deref(), Some("abc123"));
    }

    #[test]
    fn parse_register_response_error_has_no_client_key() {
        let response =
            parse_register_response(r#"{"type":"error","error":{"code":401,"message":"denied"}}"#)
                .expect("valid register error");
        assert_eq!(response.kind.as_deref(), Some("error"));
        assert_eq!(extract_client_key(&response), None);
    }

    #[test]
    fn build_request_serializes_uri_and_payload() {
        let raw = serde_json::to_string(&build_request(
            "2",
            SsapUri::AudioSetVolume,
            Some(json!({ "volume": 20 })),
        ))
        .expect("request serializes");
        let value: serde_json::Value = serde_json::from_str(&raw).expect("valid json");

        assert_eq!(value["type"], "request");
        assert_eq!(value["uri"], "ssap://audio/setVolume");
        assert_eq!(value["payload"], json!({ "volume": 20 }));
    }

    #[test]
    fn build_request_omits_payload_when_none() {
        let raw = serde_json::to_string(&build_request("3", SsapUri::SystemTurnOff, None))
            .expect("request serializes");
        assert!(!raw.contains("payload"));
    }

    #[test]
    fn parse_ssap_response_reads_payload() {
        let response = parse_ssap_response(
            r#"{"id":"3","type":"response","payload":{"volume":15,"muted":false}}"#,
        )
        .expect("valid response");
        assert_eq!(
            response.payload,
            Some(json!({ "volume": 15, "muted": false }))
        );
        assert!(response.error.is_none());
    }

    #[test]
    fn parse_ssap_response_reads_error() {
        let response = parse_ssap_response(
            r#"{"id":"4","type":"error","error":{"code":500,"message":"boom"}}"#,
        )
        .expect("valid error response");
        let error = response.error.expect("error body present");
        assert_eq!(error.code, 500);
        assert_eq!(error.message, "boom");
    }

    #[test]
    fn ws_url_default_is_legacy_port() {
        let client = WebosClient::new("192.168.1.20");
        assert_eq!(
            client.ws_url().expect("url builds"),
            "ws://192.168.1.20:3000/"
        );
    }

    #[test]
    fn ws_url_with_wss_is_modern_port() {
        let client = WebosClient::new("192.168.1.20").with_wss();
        assert_eq!(client.port(), 3001);
        assert_eq!(
            client.ws_url().expect("url builds"),
            "wss://192.168.1.20:3001/"
        );
    }

    #[test]
    fn client_defaults_are_strict_and_unkeyed() {
        let client = WebosClient::new("192.168.1.20");
        assert_eq!(client.tls_policy(), TlsPolicy::Strict);
        assert_eq!(client.client_key(), None);
        assert_eq!(client.host(), "192.168.1.20");
    }

    #[test]
    fn the_ladder_tries_the_other_port_and_then_remembers_the_one_that_worked() {
        let legacy = WebosClient::new("192.168.1.20");
        assert_eq!(
            legacy.port_candidates(),
            vec![WEBOS_WS_PORT, WEBOS_WSS_PORT]
        );

        let modern = WebosClient::new("192.168.1.20").with_wss();
        assert_eq!(
            modern.port_candidates(),
            vec![WEBOS_WSS_PORT, WEBOS_WS_PORT]
        );

        modern.remember_port(WEBOS_WSS_PORT);
        assert_eq!(modern.port_candidates(), vec![WEBOS_WSS_PORT]);

        let reset = modern.with_port(WEBOS_WS_PORT);
        assert_eq!(reset.negotiated_port(), None);
    }

    #[test]
    fn the_client_key_is_not_printable() {
        let client =
            WebosClient::new("192.168.1.20").with_client_key("140cce792ae045920e14da4daa414582");
        let rendered = format!("{client:?}");
        assert!(!rendered.contains("140cce79"), "{rendered}");
        assert!(rendered.contains("<redacted>"), "{rendered}");
    }
}
