//! A budgeted frame reader for the vendor WebSocket channels.

use futures_util::StreamExt;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Error as WsError;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

/// The concrete stream returned by `connect_async_tls_with_config` for both the
/// `ws://` and `wss://` vendor channels.
pub type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// How a [`read_until`] call ended.
#[derive(Debug)]
pub enum ReadOutcome<T> {
    /// The interpreter accepted a frame and produced `T`.
    Ready(T),
    /// The peer closed the connection before a usable frame arrived.
    Closed,
    /// The frame budget ran out before a usable frame arrived.
    BudgetExhausted,
    /// The underlying WebSocket failed.
    Failed(WsError),
}

/// Reads frames from `stream` until the interpreter accepts one, the peer closes
/// the connection, or `budget` is exhausted.
///
/// `interpret` sees every frame except Close and returns `Ok(None)` to keep
/// reading or `Ok(Some(value))` to stop with that value. Returning `Err` aborts
/// the read immediately with the caller's own error type, so a JSON or
/// protocol-level failure never has to be flattened into a transport error.
///
/// `budget` is a `&mut` counter rather than a plain `usize` so a session can
/// carry one budget across calls: each frame inspected decrements it.
///
/// The caller decides what a given [`ReadOutcome`] means — a closed socket is a
/// protocol error during a handshake but a clean end-of-stream while draining.
#[must_use = "the outcome tells you whether a reply arrived, the socket closed or the budget ran out"]
pub async fn read_until<T, E>(
    stream: &mut WsStream,
    budget: &mut usize,
    mut interpret: impl FnMut(Message) -> Result<Option<T>, E>,
) -> Result<ReadOutcome<T>, E> {
    while *budget > 0 {
        *budget -= 1;

        match stream.next().await {
            Some(Ok(message)) => {
                if message.is_close() {
                    return Ok(ReadOutcome::Closed);
                }
                if let Some(value) = interpret(message)? {
                    return Ok(ReadOutcome::Ready(value));
                }
            }
            Some(Err(error)) => return Ok(ReadOutcome::Failed(error)),
            None => return Ok(ReadOutcome::Closed),
        }
    }

    Ok(ReadOutcome::BudgetExhausted)
}
