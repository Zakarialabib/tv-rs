//! Tizen remote-channel client, session, and network-free helper functions.
//!
//! The pure helpers ([`encode_remote_name`], [`build_ws_url`],
//! [`build_channel_connect_payload`], [`build_remote_command_payload`],
//! [`parse_channel_connect_response`], [`extract_token`], [`is_unauthorized`],
//! [`build_dial_launch_url`]) contain all parsing/formatting logic and are covered
//! by network-free tests.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use futures_util::SinkExt;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio_tungstenite::connect_async_tls_with_config;
use tokio_tungstenite::tungstenite::Message;
use tv_rs_transport::{read_until, ReadOutcome, TlsPolicy, WsStream};
use url::Url;

use crate::error::{Result, TizenError};
use crate::payload::{
    parse_device_info, DeviceInfo, TizenChannelConnectData, TizenChannelConnectParams,
    TizenChannelConnectPayload, TizenChannelConnectResponse, TizenChannelEmitParams,
    TizenChannelEmitPayload, TizenChannelLaunchData, TizenDialLaunchResponse,
    TizenLegacyLaunchPayload, TizenRemoteCommand, TizenRemoteCommandParams, TizenTextCommand,
    TizenTextCommandParams,
};
use crate::types::{
    validate_remote_key_code, TizenLaunchAction, TizenRemoteKey, TIZEN_APP_ID, TIZEN_CMD_BASE64,
    TIZEN_CMD_CLICK, TIZEN_DEVICE_INFO_PATH, TIZEN_DIAL_DESC_PATH, TIZEN_DIAL_LAUNCH_PREFIX,
    TIZEN_DIAL_PORT, TIZEN_REMOTE_TYPE, TIZEN_REMOTE_TYPE_TEXT, TIZEN_REMOTE_WS_PATH,
    TIZEN_WSS_PORT, TIZEN_WS_PORT,
};

/// Default number of inbound frames inspected while awaiting a handshake reply or
/// a JSON response before giving up with [`TizenError::Timeout`].
const DEFAULT_FRAME_BUDGET: usize = 16;

/// The event the TV sends when the remote channel is established.
const EVENT_CONNECT: &str = "ms.channel.connect";

/// The event the TV sends when the user denies the on-screen pairing prompt.
const EVENT_UNAUTHORIZED: &str = "ms.channel.unauthorized";

/// The event the TV sends for channel-level errors (including stale tokens).
const EVENT_ERROR: &str = "ms.error";

/// Pairing waits for a human to accept the TV's on-screen prompt.
const PAIR_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);

/// Result of a completed `ms.channel.connect` handshake.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HandshakeInfo {
    /// Token issued by the TV, when this firmware issues one.
    pub token: Option<String>,
    /// TV name reported by the handshake, when present.
    pub tv_name: Option<String>,
}

/// Result of [`TizenClient::pair`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TizenPairing {
    /// Pairing token, if the TV issues one (pre-2018 sets may not).
    pub token: Option<String>,
    /// TV name reported during pairing, when present.
    pub tv_name: Option<String>,
}

/// Base64-encodes a remote name using the standard alphabet.
///
/// The Tizen remote channel carries the remote name in the URL query string as
/// `?name=<base64(remote-name)>`.
pub fn encode_remote_name(name: &str) -> String {
    STANDARD.encode(name.as_bytes())
}

/// Builds the Tizen remote-channel WebSocket URL.
///
/// Produces `ws://` when `tls` is `false` and `wss://` otherwise, pointing at
/// [`TIZEN_REMOTE_WS_PATH`] with the base64-encoded `remote_name` in the `name`
/// query parameter.
pub fn build_ws_url(host: &str, port: u16, tls: bool, remote_name: &str) -> Result<String> {
    let scheme = if tls { "wss" } else { "ws" };
    let encoded = encode_remote_name(remote_name);
    let url = format!("{scheme}://{host}:{port}{TIZEN_REMOTE_WS_PATH}?name={encoded}");
    Url::parse(&url)?;
    Ok(url)
}

/// Builds the `ms.channel.connect` payload used to open the remote channel.
///
/// When `token` is `None` (or empty) the token field is omitted entirely, which
/// makes the TV display the on-screen PIN prompt.
pub fn build_channel_connect_payload(
    token: Option<&str>,
    remote_name: &str,
) -> TizenChannelConnectPayload {
    TizenChannelConnectPayload {
        method: EVENT_CONNECT.to_string(),
        params: TizenChannelConnectParams {
            token: token.filter(|t| !t.is_empty()).map(str::to_string),
            data: TizenChannelConnectData {
                name: remote_name.to_string(),
                app_id: TIZEN_APP_ID.to_string(),
            },
        },
    }
}

/// Builds the `ms.remote.control` payload that presses `key`.
///
/// The `Option` field is the string `"false"`, matching the wire format used by
/// the official remote.
pub fn build_remote_command_payload(key: TizenRemoteKey) -> TizenRemoteCommand {
    TizenRemoteCommand {
        method: "ms.remote.control".to_string(),
        params: TizenRemoteCommandParams {
            cmd: TIZEN_CMD_CLICK.to_string(),
            data_of_cmd: key.as_str().to_string(),
            option: "false".to_string(),
            type_of_remote: TIZEN_REMOTE_TYPE.to_string(),
        },
    }
}

/// Builds the `ms.remote.control` payload for any valid Samsung key code.
///
/// Use this for model-specific keys (for example `KEY_CH_LIST` or channel
/// digits) that are absent from [`TizenRemoteKey`]. The code is validated with
/// [`validate_remote_key_code`] before it reaches the wire.
pub fn build_raw_remote_command_payload(code: &str) -> Result<TizenRemoteCommand> {
    validate_remote_key_code(code)?;
    Ok(TizenRemoteCommand {
        method: "ms.remote.control".to_string(),
        params: TizenRemoteCommandParams {
            cmd: TIZEN_CMD_CLICK.to_string(),
            data_of_cmd: code.to_string(),
            option: "false".to_string(),
            type_of_remote: TIZEN_REMOTE_TYPE.to_string(),
        },
    })
}

/// Builds the `ms.remote.control` payload that types `text`.
///
/// The text is base64-encoded into `Cmd` and `DataOfCmd` says so. Roku takes the
/// characters directly and webOS takes plain UTF-8; this vendor is the one that
/// encodes, which is why the three are separate implementations rather than one
/// shared `type_text`.
///
/// Empty text is refused rather than sent: a frame that types nothing is
/// indistinguishable from one that failed, and the caller asked for neither.
pub fn build_text_command_payload(text: &str) -> Result<TizenTextCommand> {
    if text.is_empty() {
        return Err(TizenError::Protocol(
            "refusing to send an empty text frame: it types nothing, and a caller \
             cannot tell that from a failure"
                .to_owned(),
        ));
    }

    use base64::Engine as _;

    Ok(TizenTextCommand {
        method: "ms.remote.control".to_string(),
        params: TizenTextCommandParams {
            cmd: base64::engine::general_purpose::STANDARD.encode(text.as_bytes()),
            data_of_cmd: TIZEN_CMD_BASE64.to_string(),
            type_of_remote: TIZEN_REMOTE_TYPE_TEXT.to_string(),
        },
    })
}

/// Builds the `ms.channel.emit` payload that launches an app through the remote
/// channel.
pub fn build_channel_launch_payload(
    app_id: &str,
    action: TizenLaunchAction,
    meta_tag: Option<&str>,
) -> TizenChannelEmitPayload {
    TizenChannelEmitPayload {
        method: "ms.channel.emit".to_string(),
        params: TizenChannelEmitParams {
            event: "ed.apps.launch".to_string(),
            to: "host".to_string(),
            data: TizenChannelLaunchData {
                app_id: app_id.to_string(),
                action_type: action.as_str().to_string(),
                meta_tag: meta_tag.map(str::to_string),
            },
        },
    }
}

/// Builds the legacy `ms.channel.launch` event used by older clients.
pub fn build_legacy_launch_payload(app_id: &str) -> TizenLegacyLaunchPayload {
    TizenLegacyLaunchPayload {
        event: "ms.channel.launch".to_string(),
        data: app_id.to_string(),
        id: 0,
    }
}

/// Parses a raw text frame into a [`TizenChannelConnectResponse`].
pub fn parse_channel_connect_response(raw: &str) -> Result<TizenChannelConnectResponse> {
    Ok(serde_json::from_str(raw)?)
}

/// Extracts the pairing token from a response, when it is present and non-empty.
pub fn extract_token(resp: &TizenChannelConnectResponse) -> Option<String> {
    resp.data
        .as_ref()
        .and_then(|data| data.token.as_ref())
        .filter(|token| !token.is_empty())
        .cloned()
}

/// Extracts the TV name from a connect response, accepting both firmware
/// spellings (`name` and `deviceName`).
pub fn extract_tv_name(resp: &TizenChannelConnectResponse) -> Option<String> {
    resp.data
        .as_ref()
        .and_then(|data| data.name.as_ref().or(data.device_name.as_ref()))
        .map(|name| name.trim())
        .filter(|name| !name.is_empty())
        .map(str::to_string)
}

/// Returns `true` when the response is the `ms.channel.unauthorized` denial event.
pub fn is_unauthorized(resp: &TizenChannelConnectResponse) -> bool {
    resp.event.as_deref() == Some(EVENT_UNAUTHORIZED)
}

/// Builds the DIAL app-launch URL `http://host:dial_port/ws/apps/{app_id}`.
pub fn build_dial_launch_url(host: &str, dial_port: u16, app_id: &str) -> Result<String> {
    let url = format!("http://{host}:{dial_port}{TIZEN_DIAL_LAUNCH_PREFIX}{app_id}");
    Url::parse(&url)?;
    Ok(url)
}

/// How long one connection attempt may take before it counts as "not listening".
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// How long the `ms.channel.connect` handshake may take once connected.
///
/// The frame reader has a budget in frames; a 2024 panel that accepts the socket and
/// then says nothing would otherwise hang the caller forever.
const DEFAULT_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

impl TizenClient {
    /// The ports to try, most likely first.
    ///
    /// Once a port has answered, only that port is tried: re-probing costs two
    /// dead connections per command on a TV that already told us which one it uses.
    fn port_candidates(&self) -> Vec<u16> {
        if let Some(port) = *self.negotiated.lock().expect("negotiated lock") {
            return vec![port];
        }
        if let Some(port) = self.pinned {
            return vec![port];
        }
        let other = if self.port == TIZEN_WSS_PORT {
            TIZEN_WS_PORT
        } else {
            TIZEN_WSS_PORT
        };
        vec![self.port, other]
    }

    fn remember_port(&self, port: u16) {
        *self.negotiated.lock().expect("negotiated lock") = Some(port);
    }

    /// The port that last answered, if any.
    #[must_use]
    pub fn negotiated_port(&self) -> Option<u16> {
        *self.negotiated.lock().expect("negotiated lock")
    }
}

/// Opens a remote channel and performs the `ms.channel.connect` handshake.
///
/// Returns the live stream and the captured handshake metadata.
pub(crate) async fn open_channel(
    client: &TizenClient,
    token: Option<&str>,
) -> Result<(WsStream, HandshakeInfo)> {
    let mut last_error: Option<TizenError> = None;

    for port in client.port_candidates() {
        let use_tls = port == TIZEN_WSS_PORT;
        let url = build_ws_url(&client.host, port, use_tls, &client.device_name)?;
        let connector = tv_rs_transport::connector(client.tls.clone(), use_tls)
            .map_err(|err| TizenError::Tls(err.to_string()))?;

        let attempt = tokio::time::timeout(
            CONNECT_TIMEOUT,
            connect_async_tls_with_config(url.as_str(), None, false, Some(connector)),
        )
        .await;

        let mut stream = match attempt {
            Ok(Ok((stream, _response))) => {
                client.remember_port(port);
                stream
            }
            // Nothing answered on this port: the other generation is worth a try.
            Ok(Err(error)) if tv_rs_transport::should_try_next_port(&error) => {
                last_error = Some(TizenError::Ws(error));
                continue;
            }
            // An answer we do not like (certificate, protocol) is not a port problem.
            Ok(Err(error)) => return Err(TizenError::Ws(error)),
            Err(_elapsed) => {
                last_error = Some(TizenError::Timeout);
                continue;
            }
        };

        let payload = build_channel_connect_payload(token, &client.device_name);
        let text = serde_json::to_string(&payload)?;
        stream.send(Message::text(text)).await?;

        let handshake = tokio::time::timeout(
            client.handshake_timeout,
            await_handshake_reply(&mut stream, DEFAULT_FRAME_BUDGET),
        )
        .await;
        return match handshake {
            Ok(captured) => Ok((stream, captured?)),
            Err(_elapsed) => Err(TizenError::Timeout),
        };
    }

    Err(last_error.unwrap_or(TizenError::Timeout))
}

/// Reads frames until the connect or unauthorized event arrives, or the budget runs out.
async fn await_handshake_reply(
    stream: &mut WsStream,
    frame_budget: usize,
) -> Result<HandshakeInfo> {
    let mut budget = frame_budget;
    let outcome = read_until(stream, &mut budget, |message| {
        if !message.is_text() {
            return Ok(None);
        }
        let resp = parse_channel_connect_response(&message.into_text()?)?;
        if is_unauthorized(&resp) || resp.event.as_deref() == Some(EVENT_ERROR) {
            return Err(TizenError::Unauthorized);
        }
        if resp.event.as_deref() == Some(EVENT_CONNECT) {
            return Ok(Some(HandshakeInfo {
                token: extract_token(&resp),
                tv_name: extract_tv_name(&resp),
            }));
        }
        Ok(None)
    })
    .await?;

    match outcome {
        ReadOutcome::Ready(info) => Ok(info),
        ReadOutcome::Closed => Err(TizenError::Protocol(
            "channel closed before the handshake completed".to_string(),
        )),
        ReadOutcome::BudgetExhausted => Err(TizenError::Timeout),
        ReadOutcome::Failed(err) => Err(TizenError::Ws(err)),
    }
}

/// A Tizen TV remote-control client.
///
/// Holds the connection target plus the pairing state, and exposes the DIAL HTTP
/// endpoints. Use the `with_*` builder methods to customise it.
#[derive(Clone)]
pub struct TizenClient {
    host: String,
    port: u16,
    dial_port: u16,
    tls: TlsPolicy,
    device_name: String,
    token: Option<String>,
    /// How long to wait for `ms.channel.connect` after opening the socket.
    handshake_timeout: Duration,
    /// Which port actually answered, once one has. Shared across clones: every
    /// session for the same TV should stop re-probing.
    negotiated: Arc<Mutex<Option<u16>>>,
    /// A port the caller has already established as the only one this panel
    /// listens on (from the quirks table). Never falls back.
    pinned: Option<u16>,
    /// Shared HTTP client for the DIAL endpoints.
    ///
    /// A `reqwest::Client` owns a connection pool and a TLS configuration; building
    /// one per launch throws both away and re-resolves DNS every time.
    http: reqwest::Client,
}

impl std::fmt::Debug for TizenClient {
    /// Renders the client without its pairing token.
    ///
    /// The token is a bearer credential: anyone who reads a log line containing it
    /// can drive the TV until the owner forgets the device on-screen.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TizenClient")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("dial_port", &self.dial_port)
            .field("tls", &self.tls)
            .field("device_name", &self.device_name)
            .field("token", &self.token.as_ref().map(|_| "<redacted>"))
            .field("negotiated_port", &self.negotiated_port())
            .finish()
    }
}

impl TizenClient {
    /// Creates a client for `host` using the legacy WS port, DIAL port 8080, the
    /// [`TlsPolicy::Strict`] policy, the default device name and no token.
    pub fn new(host: impl Into<String>) -> Self {
        Self {
            host: host.into(),
            port: TIZEN_WS_PORT,
            dial_port: TIZEN_DIAL_PORT,
            tls: TlsPolicy::default(),
            device_name: TIZEN_APP_ID.to_string(),
            token: None,
            handshake_timeout: DEFAULT_HANDSHAKE_TIMEOUT,
            negotiated: Arc::new(Mutex::new(None)),
            pinned: None,
            http: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
        }
    }

    /// Sets the WebSocket remote-channel port.
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        *self.negotiated.lock().expect("negotiated lock") = None;
        self
    }

    /// Switches to the modern TLS remote channel on [`TIZEN_WSS_PORT`].
    pub fn with_wss(mut self) -> Self {
        self.port = TIZEN_WSS_PORT;
        *self.negotiated.lock().expect("negotiated lock") = None;
        self
    }

    /// Uses `port` and **only** `port`.
    ///
    /// The port ladder exists because the generation of a panel is not known until
    /// something answers. When the caller already knows — the quirks table says
    /// `T-KTM` firmware does not listen on 8001 — trying the other port is a
    /// guaranteed timeout per command, so this removes it from the ladder instead of
    /// relying on the failure being fast.
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

    /// Sets the DIAL HTTP port.
    pub fn with_dial_port(mut self, port: u16) -> Self {
        self.dial_port = port;
        self
    }

    /// Sets the TLS certificate-validation policy.
    ///
    /// Use [`TlsPolicy::AcceptInvalid`] only as the Phase-1 interim for the
    /// playbook §10 Gap A pinning bypass; it is unsafe for production.
    pub fn with_tls_policy(mut self, tls: TlsPolicy) -> Self {
        self.tls = tls;
        self
    }

    /// Sets the human-readable device name advertised to the TV.
    pub fn with_device_name(mut self, name: impl Into<String>) -> Self {
        self.device_name = name.into();
        self
    }

    /// Sets a previously-saved pairing token for reconnects.
    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    /// Overrides the `ms.channel.connect` wait.
    ///
    /// Pairing uses 30 seconds because a human must accept the TV prompt; the
    /// default 10 seconds is for reconnects that should already be authorised.
    pub fn with_handshake_timeout(mut self, timeout: Duration) -> Self {
        self.handshake_timeout = timeout;
        self
    }

    /// Returns the TV host.
    pub fn host(&self) -> &str {
        &self.host
    }

    /// Returns the remote-channel port.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Returns the configured TLS policy.
    pub fn tls_policy(&self) -> TlsPolicy {
        self.tls.clone()
    }

    /// Returns the saved pairing token, if any.
    pub fn token(&self) -> Option<&str> {
        self.token.as_deref()
    }

    /// Launches `app_id` through the DIAL server.
    ///
    /// Sends a fire-and-forget `POST` whose body is `payload` (or empty) with
    /// `Content-Type: text/plain;charset=UTF-8`, and returns the raw response body
    /// (normally empty).
    pub async fn launch_app_via_dial(
        &self,
        app_id: &str,
        payload: Option<&str>,
    ) -> Result<TizenDialLaunchResponse> {
        let url = build_dial_launch_url(&self.host, self.dial_port, app_id)?;
        let body = payload.unwrap_or("").to_string();
        let response = self
            .http
            .post(url)
            .header(reqwest::header::CONTENT_TYPE, "text/plain;charset=UTF-8")
            .body(body)
            .send()
            .await?
            .error_for_status()?;
        let raw = response.text().await?;
        Ok(TizenDialLaunchResponse { raw })
    }

    /// Reads the set's device information, before any pairing.
    ///
    /// `GET http://<ip>:8001/api/v2/` needs no token and no auth header — it is
    /// readable before the remote channel is ever opened, which is what makes it the
    /// one place a Samsung set will tell you its hardware address. The playbook
    /// records the field, its traps and the five captures behind it (§3).
    ///
    /// # Errors
    ///
    /// Returns [`TizenError::Http`] when the set does not answer and
    /// [`TizenError::Protocol`] when it answers without a `device` object.
    pub async fn device_info(&self) -> Result<DeviceInfo> {
        let url = format!(
            "http://{}:{}{TIZEN_DEVICE_INFO_PATH}",
            self.host, TIZEN_WS_PORT
        );
        let response = self.http.get(url).send().await?.error_for_status()?;
        let body = response.text().await?;
        parse_device_info(&body)
    }

    /// Fetches the raw DIAL device description XML from [`TIZEN_DIAL_DESC_PATH`].
    ///
    /// The XML is returned verbatim; parsing it into typed device metadata lives in
    /// the `tv-rs-discovery` crate.
    pub async fn dial_device_description(&self) -> Result<String> {
        let url = format!(
            "http://{}:{}{TIZEN_DIAL_DESC_PATH}",
            self.host, self.dial_port
        );
        let response = self.http.get(url).send().await?.error_for_status()?;
        Ok(response.text().await?)
    }

    /// Launches an application through DIAL, with an optional body.
    ///
    /// `POST http://host:8080/ws/apps/{app_id}`. This is DIAL's *launch*, not
    /// "load this URL": the body is an opaque payload a player app may read as a
    /// deep link, and most ignore it. Kept separate from `open_url` on purpose — a
    /// set with no browser has no browser, and making `open_url` mean "launch an
    /// app" on one vendor is the quiet divergence the capability table exists to
    /// prevent.
    ///
    /// # Errors
    ///
    /// [`TizenError::Protocol`] when DIAL answers a non-success status, so a
    /// refused launch is not reported as one that happened.
    pub async fn dial_launch(&self, app_id: &str, payload: Option<&str>) -> Result<()> {
        let url = build_dial_launch_url(&self.host, self.dial_port, app_id)?;
        let mut request = self.http.post(url);
        if let Some(payload) = payload {
            request = request
                .header(reqwest::header::CONTENT_TYPE, "text/plain; charset=utf-8")
                .body(payload.to_owned());
        }
        let response = request.send().await?;
        if !response.status().is_success() {
            return Err(TizenError::Protocol(format!(
                "DIAL launch answered HTTP {}",
                response.status()
            )));
        }
        Ok(())
    }

    /// Runs the pairing flow without a token so the TV displays its allow prompt.
    ///
    /// Waits up to 30 seconds for a human to accept the prompt, then closes the
    /// channel. A TV that never issues a token (pre-2018 firmware) is still a
    /// successful pairing; `token` is simply `None`.
    pub async fn pair(&self) -> Result<TizenPairing> {
        let mut client = self.clone();
        client.token = None;
        client.handshake_timeout = PAIR_HANDSHAKE_TIMEOUT;
        let (mut stream, info) = open_channel(&client, None).await?;
        let _ = stream.close(None).await;
        Ok(TizenPairing {
            token: info.token,
            tv_name: info.tv_name,
        })
    }

    /// Launches an app through the remote channel's `ed.apps.launch` event.
    ///
    /// This uses the client's stored token when present. For TVs where the
    /// remote-channel launch is unavailable, call [`launch_app_via_dial`](Self::launch_app_via_dial)
    /// instead (or use the legacy event as a final fallback).
    pub async fn launch_app_via_channel(
        &self,
        app_id: &str,
        action: TizenLaunchAction,
    ) -> Result<()> {
        let mut session = TizenSession::connect(self).await?;
        session.launch_app_with_action(app_id, action).await
    }

    /// Launches an app with the older `ms.channel.launch` event.
    ///
    /// Kept for TVs that accepted the pre-`ed.apps.launch` payload. Prefer
    /// [`launch_app_via_channel`](Self::launch_app_via_channel) or DIAL.
    pub async fn launch_app_via_legacy_channel(&self, app_id: &str) -> Result<()> {
        let mut session = TizenSession::connect(self).await?;
        let payload = serde_json::to_string(&build_legacy_launch_payload(app_id))?;
        session.send_raw(&payload).await
    }
}

/// An open Tizen remote-channel session.
///
/// Wraps the connected WebSocket stream and the pairing token captured during the
/// handshake. Frames are read lazily via [`TizenSession::recv_json`], bounded by a
/// frame budget so a mute TV cannot hang the caller.
pub struct TizenSession {
    stream: WsStream,
    token: Option<String>,
    tv_name: Option<String>,
    /// Frames a single `recv_json` call may read before giving up.
    ///
    /// Per call, not per session: a session that reused one counter ran out after a
    /// dozen reads and then reported a timeout for every later command.
    frame_budget_per_read: usize,
    client: TizenClient,
}

impl TizenSession {
    /// Opens a session against `client`, performing the PIN/token handshake.
    ///
    /// Uses the token already stored on `client` (if any); otherwise the TV shows
    /// the on-screen PIN prompt. The captured token is available via
    /// [`TizenSession::token`]. A denial surfaces as [`TizenError::Unauthorized`].
    pub async fn connect(client: &TizenClient) -> Result<TizenSession> {
        let (stream, info) = open_channel(client, client.token.as_deref()).await?;
        Ok(TizenSession {
            stream,
            token: info.token,
            tv_name: info.tv_name,
            frame_budget_per_read: DEFAULT_FRAME_BUDGET,
            client: client.clone(),
        })
    }

    /// Returns the pairing token captured during the handshake, if any.
    pub fn token(&self) -> Option<&str> {
        self.token.as_deref()
    }

    /// Returns the TV name reported during the handshake, if any.
    pub fn tv_name(&self) -> Option<&str> {
        self.tv_name.as_deref()
    }

    /// Sends a remote key press as a `ms.remote.control` frame.
    pub async fn send_key(&mut self, key: TizenRemoteKey) -> Result<()> {
        let command = build_remote_command_payload(key);
        let text = serde_json::to_string(&command)?;
        self.send_raw(&text).await
    }

    /// Sends any valid Samsung `KEY_*` code, including model-specific keys that
    /// are not represented by [`TizenRemoteKey`].
    pub async fn send_key_code(&mut self, code: &str) -> Result<()> {
        let command = build_raw_remote_command_payload(code)?;
        let text = serde_json::to_string(&command)?;
        self.send_raw(&text).await
    }

    /// Launches an application through the remote channel.
    pub async fn launch_app(&mut self, app_id: &str) -> Result<()> {
        self.launch_app_with_action(app_id, TizenLaunchAction::NativeLaunch)
            .await
    }

    /// Launches an application through the remote channel with an explicit
    /// `NATIVE_LAUNCH` / `DEEP_LINK` action.
    pub async fn launch_app_with_action(
        &mut self,
        app_id: &str,
        action: TizenLaunchAction,
    ) -> Result<()> {
        let payload = serde_json::to_string(&build_channel_launch_payload(app_id, action, None))?;
        self.send_raw(&payload).await
    }

    /// Sends a raw text frame.
    pub async fn send_raw(&mut self, raw: &str) -> Result<()> {
        self.stream.send(Message::text(raw.to_string())).await?;
        Ok(())
    }

    /// Receives the next JSON text frame.
    ///
    /// Returns `Ok(None)` on a clean close, [`TizenError::Timeout`] when the frame
    /// budget is exhausted without a text frame, and [`TizenError::Protocol`] on a
    /// non-text frame that is neither Ping, Pong, nor Close.
    pub async fn recv_json(&mut self) -> Result<Option<serde_json::Value>> {
        // A fresh budget per call: this guards one read against a chatty TV, it is
        // not a lifetime quota for the session.
        let mut budget = self.frame_budget_per_read;
        let outcome = read_until(&mut self.stream, &mut budget, |message| {
            if message.is_text() {
                return Ok(Some(serde_json::from_str::<serde_json::Value>(
                    &message.into_text()?,
                )?));
            }
            if message.is_ping() || message.is_pong() {
                return Ok(None);
            }
            Err(TizenError::Protocol(
                "received a non-text frame that is not Ping/Pong/Close".to_string(),
            ))
        })
        .await?;

        match outcome {
            ReadOutcome::Ready(value) => Ok(Some(value)),
            ReadOutcome::Closed => Ok(None),
            ReadOutcome::BudgetExhausted => Err(TizenError::Timeout),
            ReadOutcome::Failed(err) => Err(TizenError::Ws(err)),
        }
    }
}

/// Connects a fresh channel and captures the token, using the session's stored
/// [`TizenClient`] state.
///
/// This is the [`crate::WebSocketHandshake`] entry point: it re-runs the
/// `ms.channel.connect` handshake against the client configuration the session was
/// created from and returns the token the TV replies with.
#[async_trait::async_trait]
impl crate::protocol::WebSocketHandshake for TizenSession {
    async fn connect_ws(&self, token: Option<&str>) -> Result<Option<String>> {
        let (_stream, info) = open_channel(&self.client, token).await?;
        Ok(info.token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn encode_remote_name_matches_known_base64() {
        assert_eq!(encode_remote_name("trae-tv"), "dHJhZS10dg==");
    }

    #[test]
    fn build_ws_url_plain_and_tls() {
        let plain = build_ws_url("192.168.1.10", TIZEN_WS_PORT, false, "tv-rs")
            .expect("plain ws url should build");
        assert!(
            plain
                .starts_with("ws://192.168.1.10:8001/api/v2/channels/samsung.remote.control?name="),
            "unexpected plain url: {plain}"
        );

        let tls = build_ws_url("192.168.1.10", TIZEN_WSS_PORT, true, "tv-rs")
            .expect("tls ws url should build");
        assert!(
            tls.starts_with("wss://192.168.1.10:8002/api/v2/channels/samsung.remote.control?name="),
            "unexpected tls url: {tls}"
        );
    }

    #[test]
    fn connect_payload_includes_token_when_present() {
        let payload = build_channel_connect_payload(Some("tok"), "tv-rs");
        let json = serde_json::to_string(&payload).expect("serialize");
        assert!(json.contains("\"method\":\"ms.channel.connect\""), "{json}");
        assert!(json.contains("\"token\":\"tok\""), "{json}");
        assert!(json.contains("\"appId\":\"tv-rs\""), "{json}");
    }

    #[test]
    fn connect_payload_omits_absent_token() {
        let payload = build_channel_connect_payload(None, "tv-rs");
        let json = serde_json::to_string(&payload).expect("serialize");
        assert!(!json.contains("token"), "token key must be absent: {json}");

        let empty = build_channel_connect_payload(Some(""), "tv-rs");
        let empty_json = serde_json::to_string(&empty).expect("serialize");
        assert!(
            !empty_json.contains("token"),
            "empty token must be omitted: {empty_json}"
        );
    }

    #[test]
    fn parse_connect_response_extracts_token() {
        let raw = r#"{"event":"ms.channel.connect","data":{"token":"12345678","clients":[{"id":"a"}]},"id":"1","result":{}}"#;
        let resp = parse_channel_connect_response(raw).expect("parse");
        assert_eq!(extract_token(&resp).as_deref(), Some("12345678"));
        assert!(!is_unauthorized(&resp));
    }

    #[test]
    fn parse_unauthorized_response() {
        let raw = r#"{"event":"ms.channel.unauthorized"}"#;
        let resp = parse_channel_connect_response(raw).expect("parse");
        assert!(is_unauthorized(&resp));
        assert_eq!(extract_token(&resp), None);
    }

    #[test]
    fn remote_command_payload_fields() {
        let command = build_remote_command_payload(TizenRemoteKey::Home);
        let value: Value = serde_json::to_value(&command).expect("to_value");
        let params = &value["params"];
        assert_eq!(value["method"], "ms.remote.control");
        assert_eq!(params["Cmd"], "Click");
        assert_eq!(params["DataOfCmd"], "KEY_HOME");
        assert_eq!(params["Option"], "false");
        assert_eq!(params["TypeOfRemote"], "SendRemoteKey");
    }

    #[test]
    fn remote_key_codes() {
        assert_eq!(TizenRemoteKey::Power.as_str(), "KEY_POWER");
        assert_eq!(TizenRemoteKey::VolumeUp.as_str(), "KEY_VOLUP");
        assert_eq!(TizenRemoteKey::FastForward.as_str(), "KEY_FF");
    }

    #[test]
    fn raw_key_codes_cover_model_specific_keys() {
        let command = build_raw_remote_command_payload("KEY_CH_LIST").expect("valid key");
        let value = serde_json::to_value(command).expect("to_value");
        assert_eq!(value["method"], "ms.remote.control");
        assert_eq!(value["params"]["DataOfCmd"], "KEY_CH_LIST");
        assert_eq!(value["params"]["Cmd"], "Click");
        assert!(build_raw_remote_command_payload("MUTE").is_err());
        assert!(build_raw_remote_command_payload("key_mute").is_err());
    }

    #[test]
    fn channel_launch_payload_uses_ed_apps_launch() {
        let payload = build_channel_launch_payload(
            "1110110",
            TizenLaunchAction::DeepLink,
            Some("https://example.test"),
        );
        let value = serde_json::to_value(payload).expect("to_value");
        assert_eq!(value["method"], "ms.channel.emit");
        assert_eq!(value["params"]["event"], "ed.apps.launch");
        assert_eq!(value["params"]["to"], "host");
        assert_eq!(value["params"]["data"]["appId"], "1110110");
        assert_eq!(value["params"]["data"]["action_type"], "DEEP_LINK");
        assert_eq!(value["params"]["data"]["metaTag"], "https://example.test");
    }

    #[test]
    fn legacy_launch_payload_keeps_old_wire_shape() {
        let value = serde_json::to_value(build_legacy_launch_payload("1110110")).expect("to_value");
        assert_eq!(value["event"], "ms.channel.launch");
        assert_eq!(value["data"], "1110110");
        assert_eq!(value["id"], 0);
    }

    #[test]
    fn connect_response_name_accepts_both_firmware_spellings() {
        let named = parse_channel_connect_response(
            r#"{"event":"ms.channel.connect","data":{"token":"t","name":"Den TV"}}"#,
        )
        .expect("parse");
        assert_eq!(extract_tv_name(&named).as_deref(), Some("Den TV"));

        let device_named = parse_channel_connect_response(
            r#"{"event":"ms.channel.connect","data":{"deviceName":"  Kitchen TV  "}}"#,
        )
        .expect("parse");
        assert_eq!(
            extract_tv_name(&device_named).as_deref(),
            Some("Kitchen TV")
        );
    }

    #[test]
    fn dial_launch_url_shape() {
        let url = build_dial_launch_url("127.0.0.1", 8080, "YouTube").expect("url");
        assert_eq!(url, "http://127.0.0.1:8080/ws/apps/YouTube");
    }

    #[test]
    fn tls_policy_default_is_strict() {
        assert_eq!(TlsPolicy::default(), TlsPolicy::Strict);
    }

    #[tokio::test]
    async fn launch_app_via_dial_hits_mock_server() {
        use wiremock::matchers::method;
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(201))
            .mount(&server)
            .await;

        let client = TizenClient::new("127.0.0.1").with_dial_port(server.address().port());
        let response = client
            .launch_app_via_dial("YouTube", None)
            .await
            .expect("mock launch should succeed");
        assert_eq!(response.raw, "");
    }

    #[test]
    fn a_pinned_port_is_the_only_candidate() {
        let client = TizenClient::new("192.168.1.10")
            .with_wss()
            .pinned_port(8002);
        assert_eq!(client.pinned_port_number(), Some(8002));
        assert_eq!(client.port_candidates(), vec![8002], "no fallback to 8001");

        // Without a pin the ladder still holds both generations.
        let ladder = TizenClient::new("192.168.1.10").with_wss();
        assert_eq!(ladder.port_candidates(), vec![8002, 8001]);
    }

    #[test]
    fn the_ladder_tries_the_other_port_and_then_remembers_the_one_that_worked() {
        let legacy = TizenClient::new("192.168.1.10");
        assert_eq!(
            legacy.port_candidates(),
            vec![TIZEN_WS_PORT, TIZEN_WSS_PORT]
        );

        let modern = TizenClient::new("192.168.1.10").with_wss();
        assert_eq!(
            modern.port_candidates(),
            vec![TIZEN_WSS_PORT, TIZEN_WS_PORT]
        );

        modern.remember_port(TIZEN_WS_PORT);
        assert_eq!(
            modern.port_candidates(),
            vec![TIZEN_WS_PORT],
            "a remembered port is not re-probed"
        );
        assert_eq!(modern.negotiated_port(), Some(TIZEN_WS_PORT));

        let reset = modern.clone();
        let reset = reset.with_port(TIZEN_WSS_PORT);
        assert_eq!(
            reset.negotiated_port(),
            None,
            "reconfiguration forgets the port"
        );
    }

    #[test]
    fn the_pairing_token_is_not_printable() {
        let client = TizenClient::new("192.168.1.10").with_token("s3cr3t-token-value");
        let rendered = format!("{client:?}");
        assert!(!rendered.contains("s3cr3t"), "{rendered}");
        assert!(rendered.contains("<redacted>"), "{rendered}");
    }
    /// The text frame's shape is what the playbook records: base64 in `Cmd`, the
    /// literal `base64` in `DataOfCmd`, `SendInputString` in `TypeOfRemote`, and — the
    /// detail that makes this its own type — **no `Option` field**.
    #[test]
    fn the_text_frame_is_the_documented_shape() {
        let payload = build_text_command_payload("hello").expect("non-empty");
        let json = serde_json::to_value(&payload).expect("serializes");

        assert_eq!(json["method"], "ms.remote.control");
        assert_eq!(
            json["params"]["Cmd"], "aGVsbG8=",
            "base64 of the UTF-8 bytes"
        );
        assert_eq!(json["params"]["DataOfCmd"], "base64");
        assert_eq!(json["params"]["TypeOfRemote"], "SendInputString");
        assert!(
            json["params"].get("Option").is_none(),
            "the text frame carries no Option, unlike the key frame: {json}"
        );
    }

    /// UTF-8 is encoded as bytes, so a non-ASCII string survives the round trip —
    /// the set decides whether it can display it, not this client.
    #[test]
    fn non_ascii_text_is_encoded_as_its_utf8_bytes() {
        let payload = build_text_command_payload("caf\u{e9}").expect("non-empty");
        let json = serde_json::to_value(&payload).expect("serializes");
        let encoded = json["params"]["Cmd"].as_str().expect("a string");
        use base64::Engine as _;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .expect("valid base64");
        assert_eq!(String::from_utf8(decoded).expect("utf-8"), "caf\u{e9}");
    }

    /// An empty frame types nothing, and a caller cannot tell that from a failure.
    #[test]
    fn an_empty_string_is_refused_rather_than_sent() {
        assert!(build_text_command_payload("").is_err());
    }
}
