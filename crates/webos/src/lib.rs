//! `tv-rs-webos` — LG webOS SSAP control over the JSON-RPC WebSocket transport.
//!
//! LG webOS exposes the SSAP (Second Screen Application Protocol) as a JSON-RPC-ish
//! WebSocket service: unencrypted on port `3000` and TLS-wrapped on port `3001`. The
//! first connection triggers an on-screen "Allow?" prompt and the TV returns a
//! `client-key` that should be persisted and reused on later connections.
//!
//! # Example
//!
//! ```no_run
//! use tv_rs_webos::{load_client_key, save_client_key, WebosClient};
//!
//! # async fn run() -> tv_rs_webos::Result<()> {
//! let mut client = WebosClient::new("192.168.1.20").with_device_name("tv-rs");
//!
//! // First run: pair (triggers the on-screen prompt) and persist the issued key.
//! let pairing = client.pair().await?;
//! if let Some(key) = pairing.client_key.as_deref() {
//!     save_client_key("webos-client-key.json", key).await?;
//! }
//!
//! // Later runs: reuse the stored key so the TV does not prompt again.
//! if let Some(key) = load_client_key("webos-client-key.json").await? {
//!     client = client.with_client_key(key);
//! }
//! let mut session = client.connect().await?;
//! session.set_volume(20).await?;
//! session.launch_app("netflix").await?;
//! # Ok(())
//! # }
//! ```
//!
//! The TLS certificate-pinning work needed for reliable port `3001` use against
//! self-signed TV certificates is deferred to Phase 3 (`docs/02-playbook.md` §10,
//! "TLS certificate pinning"); see [`TlsPolicy`].

// The public error enums embed `tokio_tungstenite`'s error, which is large by
// construction. Boxing it would break the #[from] conversions callers rely on,
// and these are cold paths where the extra size costs nothing measurable.
#![forbid(unsafe_code)]
#![allow(clippy::result_large_err)]

pub mod auth;
pub mod client;
pub mod error;
pub mod payload;
pub mod pointer;
pub mod pool;
pub mod protocol;
pub mod types;

pub use auth::WebosAuthProvider;
pub use client::{
    build_register_request, build_request, extract_client_key, load_client_key,
    parse_register_response, parse_ssap_response, save_client_key, WebosClient, WebosPairing,
    WebosSession,
};
pub use error::{Result, WebosError};
pub use payload::{
    SsapErrorBody, SsapManifest, SsapRegisterPayload, SsapRegisterRequest, SsapRegisterResponse,
    SsapRegisterResponsePayload, SsapRequest, SsapResponse,
};
pub use pointer::{
    extract_socket_path, BoxedPointerFactory, LivePointerFactory, PointerFactory, PointerFrame,
    PointerSocket, PointerWire,
};
pub use pool::{
    BoxedSessionFactory, DynSessionPool, LiveSessionFactory, WebosSessionFactory, WebosSessionPool,
    WebosSessionWire,
};

/// Counting test doubles for the session pool.
///
/// Behind the `testkit` feature so a released build carries no test code: a
/// downstream crate that wants to prove it reuses one session enables it.
#[cfg(feature = "testkit")]
pub use pool::testkit;
pub use protocol::SsapController;
pub use tv_rs_transport::TlsPolicy;
pub use types::{
    InstalledApp, SsapUri, SSAP_URI_PREFIX, WEBOS_CLIENT_KEY_FILE, WEBOS_DEFAULT_PERMISSIONS,
    WEBOS_PAIRING_TYPE_PROMPT, WEBOS_WSS_PORT, WEBOS_WS_PORT,
};
