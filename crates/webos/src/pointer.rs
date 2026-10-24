//! The green-lit pointer channel: a second WebSocket with plain-text frames.
//!
//! `com.webos.service.networkinput/getPointerInputSocket` (on the main SSAP
//! socket) answers with a `socketPath` that is already a complete WebSocket
//! URL — a per-session path such as
//! `wss://<host>:3001/resources/<hash>/netinput.pointer.sock`. Dial that URL
//! directly and send plain-text ASCII frames, one per message, in the form
//!
//! ```text
//! type:button\nname:HOME\n\n
//! type:click\n\n
//! type:move\ndx:40\ndy:-10\ndown:0\n\n
//! type:scroll\ndx:0\ndy:-3\n\n
//! ```
//!
//! `down:0` is a plain move, `down:1` a press-and-drag. The frame is always
//! terminated by a blank line (a trailing `\n` after the last field), contains
//! no `\r`, and the TV never acknowledges anything — every send is
//! fire-and-forget. Before the socket is torn down the last write needs a short
//! grace period or the TV drops it unprocessed.
//!
//! Provenance: measured on webOS 23 (`d-gubert/lgtv-remote` PROTOCOL.md §6-7),
//! and confirmed by four independent implementations (`lgtv2` index.js,
//! `pywebostv` controls.py, `aiopylgtv` webos_client.py, and the droidactor
//! field writeup). The pointer channel accepts no text/keyboard frames — text
//! entry stays on `com.webos.service.ime/*` over the main socket.

use std::time::Duration;

use futures_util::SinkExt;
use tokio_tungstenite::connect_async_tls_with_config;
use tokio_tungstenite::tungstenite::Message;
use tv_rs_transport::{connector, WsStream};

use crate::error::{Result, WebosError};

/// How long one pointer-socket connection attempt may take.
const DIAL_TIMEOUT: Duration = Duration::from_secs(5);

/// How long to idle between the last pointer frame and the close frame.
///
/// Frames that land too close to the close frame are dropped unprocessed by
/// the TV; without this grace, the final press of a sequence can vanish.
const CLOSE_GRACE: Duration = Duration::from_millis(120);

/// One plain-text frame for the pointer socket.
///
/// Button names are case-sensitive on the wire (`ENTER`, not `OK`; `HOME`,
/// `BACK`, `VOLUMEUP`, …). The full vocabulary the pointer channel accepts is
/// the SSAP key-code alias set kept in `tv-rs-core`'s key map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerFrame<'a> {
    /// A single press-and-release of `name` — one atomic frame.
    Button(&'a str),
    /// "OK" at the current pointer position.
    Click,
    /// Relative pointer move; `down` turns the move into press-and-drag.
    ///
    /// `down` is always encoded (`0` or `1`): every source-checked
    /// implementation either emits it or tolerates its absence, and emitting
    /// it is the form all four agree on.
    Move {
        /// Horizontal delta in pointer units.
        dx: i32,
        /// Vertical delta in pointer units.
        dy: i32,
        /// `true` keeps the pointer pressed while moving.
        down: bool,
    },
    /// Relative scroll delta.
    Scroll {
        /// Horizontal scroll delta.
        dx: i32,
        /// Vertical scroll delta.
        dy: i32,
    },
}

impl PointerFrame<'_> {
    /// Encodes this frame exactly as the TV expects: a `type:` header line,
    /// then `key:value` lines, terminated by a blank line — ASCII only.
    #[must_use]
    pub fn encode(&self) -> String {
        match self {
            Self::Button(name) => format!("type:button\nname:{name}\n\n"),
            Self::Click => "type:click\n\n".to_string(),
            Self::Move { dx, dy, down } => {
                format!("type:move\ndx:{dx}\ndy:{dy}\ndown:{}\n\n", u8::from(*down))
            }
            Self::Scroll { dx, dy } => format!("type:scroll\ndx:{dx}\ndy:{dy}\n\n"),
        }
    }
}

/// Extracts the pointer-socket URL from a `getPointerInputSocket` payload.
///
/// The `socketPath` the TV returns is already a complete WebSocket URL; it is
/// returned verbatim. Only the scheme is validated, because a non-`ws`/`wss`
/// value is a protocol error the dialer cannot recover from.
pub fn extract_socket_path(payload: &serde_json::Value) -> Result<String> {
    let path = payload
        .get("socketPath")
        .and_then(serde_json::Value::as_str)
        .filter(|path| !path.is_empty())
        .ok_or_else(|| {
            WebosError::Protocol(
                "getPointerInputSocket response missing non-empty 'socketPath'".to_string(),
            )
        })?;
    if !(path.starts_with("ws://") || path.starts_with("wss://")) {
        return Err(WebosError::Protocol(format!(
            "pointer socketPath is not a WebSocket URL: {path}"
        )));
    }
    Ok(path.to_string())
}

/// A live pointer-socket session: one TV, one connection, frame-out only.
pub struct PointerSocket {
    stream: WsStream,
}

impl PointerSocket {
    /// Dials the pointer socket at the complete `socketPath` URL and returns a
    /// live connection.
    ///
    /// No `client-key` or extra headers are needed — the path itself is the
    /// session ticket, issued over the main SSAP connection. TLS follows the
    /// same [`TlsPolicy`](crate::TlsPolicy) as the main socket (webOS serves
    /// the pointer path on the self-signed certificate too).
    ///
    /// Ask for the path over an already-registered main session:
    ///
    /// ```no_run
    /// # async fn demo() -> tv_rs_webos::Result<()> {
    /// use tv_rs_webos::{extract_socket_path, PointerFrame, PointerSocket, SsapUri, WebosClient};
    ///
    /// let client = WebosClient::new("192.168.1.20").with_wss().with_client_key("key");
    /// let mut session = client.connect().await?;
    /// let payload = session
    ///     .request(SsapUri::NetworkInputGetPointerSocket, None)
    ///     .await?;
    /// let path = extract_socket_path(&payload)?;
    /// let mut pointer = PointerSocket::connect(client.tls_policy(), &path).await?;
    /// pointer.send(&PointerFrame::Button("HOME")).await?;
    /// pointer.close().await?;
    /// # Ok(()) }
    /// ```
    pub async fn connect(tls_policy: crate::TlsPolicy, socket_path: &str) -> Result<Self> {
        let is_wss = socket_path.starts_with("wss://");
        let connector = connector(tls_policy, is_wss).map_err(|error| {
            WebosError::Protocol(format!("failed to build TLS connector: {error}"))
        })?;

        let attempt = tokio::time::timeout(
            DIAL_TIMEOUT,
            connect_async_tls_with_config(socket_path, None, false, Some(connector)),
        )
        .await;

        match attempt {
            Ok(Ok((stream, _response))) => Ok(Self { stream }),
            Ok(Err(error)) => Err(WebosError::Ws(error)),
            Err(_elapsed) => Err(WebosError::Timeout),
        }
    }

    /// Sends one frame, fire-and-forget.
    ///
    /// The TV never acknowledges pointer frames; a successful send means the
    /// frame was written, not that the TV acted on it.
    pub async fn send(&mut self, frame: &PointerFrame<'_>) -> Result<()> {
        self.send_frame(frame.encode()).await
    }

    /// Closes the pointer socket, giving the last frame the grace it needs.
    pub async fn close(mut self) -> Result<()> {
        tokio::time::sleep(CLOSE_GRACE).await;
        self.stream.send(Message::Close(None)).await?;
        Ok(())
    }
}

/// A live pointer connection, as the session pool needs to see it.
///
/// Implemented for [`PointerSocket`]; the trait exists for the same reason
/// [`WebosSessionWire`](crate::pool::WebosSessionWire) does — so the pool's
/// reuse and reopen-once behaviour can be proven without a television. Frames
/// arrive already encoded, so the trait method carries no lifetime.
#[async_trait::async_trait]
pub trait PointerWire: Send {
    /// Writes one encoded frame; the television never acknowledges it.
    async fn send_frame(&mut self, frame: String) -> Result<()>;
}

#[async_trait::async_trait]
impl PointerWire for PointerSocket {
    async fn send_frame(&mut self, frame: String) -> Result<()> {
        self.stream.send(Message::text(frame)).await?;
        Ok(())
    }
}

/// Opens pointer sockets for the pool.
#[async_trait::async_trait]
pub trait PointerFactory: Send + Sync {
    /// Dials the complete `socketPath` URL a television returned.
    async fn open(
        &self,
        tls_policy: crate::TlsPolicy,
        socket_path: &str,
    ) -> Result<Box<dyn PointerWire>>;
}

/// The production factory: a real WebSocket dial to the TV's pointer path.
#[derive(Debug, Clone, Copy, Default)]
pub struct LivePointerFactory;

#[async_trait::async_trait]
impl PointerFactory for LivePointerFactory {
    async fn open(
        &self,
        tls_policy: crate::TlsPolicy,
        socket_path: &str,
    ) -> Result<Box<dyn PointerWire>> {
        Ok(Box::new(
            PointerSocket::connect(tls_policy, socket_path).await?,
        ))
    }
}

/// A pointer factory behind a box, so a pool can hold one it does not name.
///
/// The session-pool equivalent is [`BoxedSessionFactory`](crate::pool::BoxedSessionFactory);
/// both exist so a facade can store one pool type while a test or a different
/// transport supplies its own factory.
pub type BoxedPointerFactory = Box<dyn PointerFactory + Send + Sync>;

#[async_trait::async_trait]
impl PointerFactory for BoxedPointerFactory {
    async fn open(
        &self,
        tls_policy: crate::TlsPolicy,
        socket_path: &str,
    ) -> Result<Box<dyn PointerWire>> {
        (**self).open(tls_policy, socket_path).await
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::extract_socket_path;
    use super::PointerFrame;
    use crate::error::Result;

    #[test]
    fn button_encodes_the_exact_wire_format() {
        assert_eq!(
            PointerFrame::Button("HOME").encode(),
            "type:button\nname:HOME\n\n"
        );
    }

    #[test]
    fn button_names_are_not_touched_by_the_encoder() {
        assert_eq!(
            PointerFrame::Button("VOLUMEUP").encode(),
            "type:button\nname:VOLUMEUP\n\n"
        );
    }

    #[test]
    fn click_encodes_a_bare_frame() {
        assert_eq!(PointerFrame::Click.encode(), "type:click\n\n");
    }

    #[test]
    fn move_always_emits_the_down_flag() {
        assert_eq!(
            PointerFrame::Move {
                dx: 40,
                dy: -10,
                down: false
            }
            .encode(),
            "type:move\ndx:40\ndy:-10\ndown:0\n\n"
        );
        assert_eq!(
            PointerFrame::Move {
                dx: 12,
                dy: -4,
                down: true
            }
            .encode(),
            "type:move\ndx:12\ndy:-4\ndown:1\n\n"
        );
    }

    #[test]
    fn scroll_encodes_both_axes() {
        assert_eq!(
            PointerFrame::Scroll { dx: 0, dy: -3 }.encode(),
            "type:scroll\ndx:0\ndy:-3\n\n"
        );
    }

    #[test]
    fn socket_path_is_extracted_verbatim() -> Result<()> {
        let payload = json!({
            "socketPath": "wss://192.168.1.50:3001/resources/<hash>/netinput.pointer.sock"
        });
        assert_eq!(
            extract_socket_path(&payload)?,
            "wss://192.168.1.50:3001/resources/<hash>/netinput.pointer.sock"
        );
        Ok(())
    }

    #[test]
    fn a_missing_or_empty_socket_path_is_a_protocol_error() {
        assert!(extract_socket_path(&json!({})).is_err());
        assert!(extract_socket_path(&json!({ "socketPath": "" })).is_err());
        assert!(extract_socket_path(&json!({ "socketPath": null })).is_err());
    }

    #[test]
    fn a_non_websocket_socket_path_is_a_protocol_error() {
        assert!(extract_socket_path(&json!({ "socketPath": "/resources/abc" })).is_err());
        assert!(extract_socket_path(&json!({ "socketPath": "tcp://tv:9999" })).is_err());
    }
}
