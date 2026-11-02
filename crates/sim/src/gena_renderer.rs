//! DLNA/GENA mock renderer for integration testing.
//!
//! The subscription surface the plan calls G18 is driven by two HTTP paths
//! `wiremock` cannot close alone: the non-standard `SUBSCRIBE`/`UNSUBSCRIBE`
//! verbs against the renderer's `eventSubURL`, and the renderer's **reverse**
//! NOTIFY delivery to the subscriber's callback URL — the one direction where
//! the renderer is the HTTP client. This module provides both halves:
//!
//! - [`MockGenaRenderer`] answers the recorded GENA verb table
//!   (`docs/02-playbook.md` §7 / `tv_rs_dlna::gena_client` docs):
//!   `SUBSCRIBE` with `CALLBACK`+`NT` establishes a subscription and returns
//!   `SID` + `TIMEOUT`; `SUBSCRIBE` with `SID` renews the same subscription;
//!   `UNSUBSCRIBE` with `SID` cancels it. A renew that also carries
//!   `CALLBACK` is answered `400 Bad Request` — the protocol error the client
//!   documentation warns about. [`MockGenaRenderer::deliver_notify`] then
//!   makes the mock act as the HTTP client, POSTing a `NOTIFY` request to the
//!   callback URL the subscriber registered.
//! - [`MockNotifySink`] is the smallest caller-owned callback endpoint: it
//!   accepts any method (GENA NOTIFY is a method, not POST), records the SID
//!   and body of every delivery, and answers `200` — exactly the shape the
//!   `SubscriptionHandle`'s host application implements.
//!
//! Together they close the loop the G18 tests could only cover pure: real
//! `SUBSCRIBE` over real HTTP, a real NOTIFY delivered in the other
//! direction, and the full path into `ingest_notify` and the event stream.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

/// Everything the mock renderer observed, in arrival order.
#[derive(Debug, Default, Clone)]
pub struct GenaRecord {
    /// `CALLBACK` header values received on establishing SUBSCRIBEs.
    pub callbacks: Vec<String>,
    /// `SID` values presented on renew attempts.
    pub renewals: Vec<String>,
    /// `SID` values presented on UNSUBSCRIBEs.
    pub unsubscribes: Vec<String>,
    /// How many renew requests also carried `CALLBACK` (the protocol error).
    pub renew_with_callback_errors: usize,
}

/// A mock renderer's GENA endpoints: SUBSCRIBE / renew / UNSUBSCRIBE, plus
/// the ability to deliver NOTIFYs to the subscriber's callback URL.
pub struct MockGenaRenderer {
    local_addr: SocketAddr,
    shutdown_tx: Option<oneshot::Sender<()>>,
    server_task: Option<JoinHandle<()>>,
    record: Arc<Mutex<GenaRecord>>,
    /// The callback URL each SID registered (so deliver_notify knows where).
    callbacks_by_sid: Arc<Mutex<Vec<(String, String)>>>,
    client: reqwest::Client,
}

impl MockGenaRenderer {
    /// Starts the mock on an ephemeral port.
    pub async fn start(bind_addr: &str) -> Result<Self, std::io::Error> {
        let listener = TcpListener::bind(bind_addr).await?;
        let local_addr = listener.local_addr()?;
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();

        let record = Arc::new(Mutex::new(GenaRecord::default()));
        let callbacks_by_sid = Arc::new(Mutex::new(Vec::new()));
        let record_for_task = Arc::clone(&record);
        let sids_for_task = Arc::clone(&callbacks_by_sid);

        let server_task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => break,
                    accept_res = listener.accept() => {
                        match accept_res {
                            Ok((socket, _)) => {
                                let record = Arc::clone(&record_for_task);
                                let sids = Arc::clone(&sids_for_task);
                                tokio::spawn(async move {
                                    let _ = serve_gen_request(socket, record, sids).await;
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
            record,
            callbacks_by_sid,
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .expect("mock reqwest client"),
        })
    }

    /// The bound address, e.g. `127.0.0.1:49152`.
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// The event subscription URL a `Renderer` carries
    /// (`Renderer.event_sub_url`).
    pub fn event_sub_url(&self) -> String {
        format!("http://{}/dev/avtransport/event", self.local_addr)
    }

    /// Every establishing SUBSCRIBE's `CALLBACK` header, in arrival order.
    pub fn callbacks(&self) -> Vec<String> {
        self.record.lock().expect("record lock").callbacks.clone()
    }

    /// Every renew's `SID`, in arrival order.
    pub fn renewals(&self) -> Vec<String> {
        self.record.lock().expect("record lock").renewals.clone()
    }

    /// Every UNSUBSCRIBE's `SID`, in arrival order.
    pub fn unsubscribes(&self) -> Vec<String> {
        self.record
            .lock()
            .expect("record lock")
            .unsubscribes
            .clone()
    }

    /// Delivers a NOTIFY to the callback URL the subscriber registered for
    /// `sid` (the renderer is the HTTP client in this direction).
    ///
    /// Returns `false` when no subscription with that SID exists.
    pub async fn deliver_notify(&self, sid: &str, body: &str) -> bool {
        let target = {
            let map = self.callbacks_by_sid.lock().expect("sid map lock");
            map.iter()
                .rev()
                .find(|(registered, _)| registered == sid)
                .map(|(_, url)| url.clone())
        };
        let Some(callback_url) = target else {
            return false;
        };
        let response = self
            .client
            .request(
                reqwest::Method::from_bytes(b"NOTIFY").expect("NOTIFY is a valid method"),
                &callback_url,
            )
            .header("SID", sid)
            .header("NT", "upnp:event")
            .header("NTS", "upnp:propchange")
            .header("CONTENT-TYPE", "text/xml; charset=\"utf-8\"")
            .body(body.to_string())
            .send()
            .await;
        matches!(response, Ok(resp) if resp.status().is_success())
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

impl Drop for MockGenaRenderer {
    fn drop(&mut self) {
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }
        if let Some(server_task) = self.server_task.take() {
            server_task.abort();
        }
    }
}

/// The smallest caller-owned NOTIFY callback endpoint: records every
/// delivery's SID and body, answers `200`.
pub struct MockNotifySink {
    local_addr: SocketAddr,
    shutdown_tx: Option<oneshot::Sender<()>>,
    server_task: Option<JoinHandle<()>>,
    deliveries: Arc<Mutex<Vec<(String, String)>>>,
}

impl MockNotifySink {
    /// Starts the sink on an ephemeral port.
    pub async fn start(bind_addr: &str) -> Result<Self, std::io::Error> {
        let listener = TcpListener::bind(bind_addr).await?;
        let local_addr = listener.local_addr()?;
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();

        let deliveries = Arc::new(Mutex::new(Vec::new()));
        let deliveries_for_task = Arc::clone(&deliveries);

        let server_task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => break,
                    accept_res = listener.accept() => {
                        match accept_res {
                            Ok((socket, _)) => {
                                let deliveries = Arc::clone(&deliveries_for_task);
                                tokio::spawn(async move {
                                    let _ = serve_notify_delivery(socket, deliveries).await;
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
            deliveries,
        })
    }

    /// The bound address, e.g. `127.0.0.1:49152`.
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// The callback URL a subscriber registers with the renderer.
    pub fn callback_url(&self) -> String {
        format!("http://{}/notify", self.local_addr)
    }

    /// Every `(SID, body)` delivery, in arrival order.
    pub fn deliveries(&self) -> Vec<(String, String)> {
        self.deliveries.lock().expect("deliveries lock").clone()
    }

    /// Waits until at least `n` deliveries arrived (bounded poll).
    pub async fn wait_for_deliveries(&self, n: usize, budget: Duration) -> Vec<(String, String)> {
        let deadline = tokio::time::Instant::now() + budget;
        while tokio::time::Instant::now() < deadline {
            let got = self.deliveries();
            if got.len() >= n {
                return got;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        self.deliveries()
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

impl Drop for MockNotifySink {
    fn drop(&mut self) {
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }
        if let Some(server_task) = self.server_task.take() {
            server_task.abort();
        }
    }
}

/// A parsed HTTP request head (method, path, headers, and where the body starts).
struct RequestHead {
    method: String,
    headers: Vec<(String, String)>,
}

fn header<'a>(head: &'a RequestHead, name: &str) -> Option<&'a str> {
    head.headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

/// Reads one HTTP/1.1 request (head + content-length body) from the socket.
async fn read_request<S: AsyncRead + Unpin>(socket: &mut S) -> Option<(RequestHead, String)> {
    // The head is small; read until the blank line with a bounded buffer.
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    let head_end = loop {
        let n = socket.read(&mut chunk).await.ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(pos) = find_double_crlf(&buf) {
            break pos;
        }
        if buf.len() > 64 * 1024 {
            return None;
        }
    };

    let head_text = String::from_utf8_lossy(&buf[..head_end]).to_string();
    let mut lines = head_text.split("\r\n");
    let request_line = lines.next()?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next()?.to_string();
    // The path is not consulted: the mock answers every path on the port the
    // way a renderer answers its one event endpoint.
    let _path = parts.next()?.to_string();

    let mut headers = Vec::new();
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            headers.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    let head = RequestHead { method, headers };

    let content_length: usize = header(&head, "CONTENT-LENGTH")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let mut body = buf[head_end + 4..].to_vec();
    while body.len() < content_length {
        let n = socket.read(&mut chunk).await.ok()?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    body.truncate(content_length);
    Some((head, String::from_utf8_lossy(&body).to_string()))
}

fn find_double_crlf(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

async fn write_response<S: AsyncWrite + Unpin>(
    socket: &mut S,
    status: &str,
    headers: &[(&str, &str)],
) -> std::io::Result<()> {
    let mut response = format!("HTTP/1.1 {status}\r\n");
    for (k, v) in headers {
        response.push_str(&format!("{k}: {v}\r\n"));
    }
    response.push_str("SERVER: tv-rs-sim/1.0 UPnP/1.0 MockGenaRenderer/1.0\r\n");
    response.push_str("CONTENT-LENGTH: 0\r\n\r\n");
    socket.write_all(response.as_bytes()).await
}

/// Answers one GENA request against the renderer's event endpoint.
async fn serve_gen_request<S>(
    mut socket: S,
    record: Arc<Mutex<GenaRecord>>,
    sids: Arc<Mutex<Vec<(String, String)>>>,
) -> std::io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let Some((head, _body)) = read_request(&mut socket).await else {
        return Ok(());
    };

    let callback = header(&head, "CALLBACK").map(str::to_string);
    let sid = header(&head, "SID").map(str::to_string);

    match (head.method.as_str(), sid, callback) {
        // Establish: CALLBACK + NT, no SID. Returns a fresh SID.
        // The CALLBACK header is angle-bracketed per the UPnP spec
        // (`<http://host/notify>`); strip the brackets for delivery.
        ("SUBSCRIBE", None, Some(callback)) => {
            let callback = callback
                .trim()
                .trim_start_matches('<')
                .trim_end_matches('>')
                .to_string();
            let sid = format!("uuid:mock-sid-{}", {
                let mut r = record.lock().expect("record lock");
                r.callbacks.push(callback.clone());
                r.callbacks.len()
            });
            sids.lock()
                .expect("sid map lock")
                .push((sid.clone(), callback));
            write_response(
                &mut socket,
                "200 OK",
                &[("SID", sid.as_str()), ("TIMEOUT", "Second-1800")],
            )
            .await
        }
        // Renew: SID, no CALLBACK. Same SID comes back.
        ("SUBSCRIBE", Some(sid), None) => {
            record
                .lock()
                .expect("record lock")
                .renewals
                .push(sid.clone());
            write_response(
                &mut socket,
                "200 OK",
                &[("SID", sid.as_str()), ("TIMEOUT", "Second-1800")],
            )
            .await
        }
        // The documented protocol error: CALLBACK and SID together.
        ("SUBSCRIBE", Some(_sid), Some(_)) => {
            record
                .lock()
                .expect("record lock")
                .renew_with_callback_errors += 1;
            write_response(&mut socket, "400 Bad Request", &[]).await
        }
        ("UNSUBSCRIBE", Some(sid), _) => {
            record.lock().expect("record lock").unsubscribes.push(sid);
            write_response(&mut socket, "200 OK", &[]).await
        }
        // Anything else is not GENA; refuse loudly.
        _ => write_response(&mut socket, "405 Method Not Allowed", &[]).await,
    }
}

/// Receives one NOTIFY delivery into the sink.
async fn serve_notify_delivery<S>(
    mut socket: S,
    deliveries: Arc<Mutex<Vec<(String, String)>>>,
) -> std::io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let Some((head, body)) = read_request(&mut socket).await else {
        return Ok(());
    };
    let sid = header(&head, "SID").unwrap_or_default().to_string();
    deliveries
        .lock()
        .expect("deliveries lock")
        .push((sid, body));
    write_response(&mut socket, "200 OK", &[]).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn renderer_and_sink_bind_and_shut_down() {
        let renderer = MockGenaRenderer::start("127.0.0.1:0").await.expect("bind");
        let sink = MockNotifySink::start("127.0.0.1:0").await.expect("bind");
        assert!(renderer.callbacks().is_empty());
        assert!(sink.deliveries().is_empty());
        assert!(renderer.local_addr().port() > 0);
        assert!(sink.local_addr().port() > 0);
        renderer.shutdown().await;
        sink.shutdown().await;
    }
}
