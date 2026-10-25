//! `tv-rs-tizen` — Samsung Tizen WebSocket control and DIAL app launching.
//!
//! This crate implements the Phase-1 Tizen remote channel (the legacy unencrypted
//! WebSocket on port 8001 and the modern TLS WebSocket on port 8002) plus the DIAL
//! launcher on port 8080. See `docs/01-playbook.md` §3 for the protocol facts and
//! §10 for the reverse-engineering gaps that are intentionally deferred.
//!
//! Blocked capabilities (TLS certificate-pinning bypass, enterprise token math and
//! AES-GCM encrypted framing) surface as [`TizenError::Unsupported`]; this crate
//! never panics in library code.
//!
//! # Example
//!
//! ```no_run
//! use tv_rs_tizen::{TizenClient, TizenRemoteKey, TizenSession};
//!
//! # async fn run() -> tv_rs_tizen::Result<()> {
//! let client = TizenClient::new("192.168.1.10");
//! let mut session = TizenSession::connect(&client).await?;
//! // Persist the token the TV returned and reuse it on the next connect.
//! let _token = session.token().map(str::to_string);
//! session.send_key(TizenRemoteKey::Home).await?;
//! # Ok(())
//! # }
//! ```

// The public error enums embed `tokio_tungstenite`'s error, which is large by
// construction. Boxing it would break the #[from] conversions callers rely on,
// and these are cold paths where the extra size costs nothing measurable.
#![forbid(unsafe_code)]
#![allow(clippy::result_large_err)]

pub mod apps;
pub mod auth;
pub mod client;
pub mod discovery;
pub mod error;
pub mod payload;
pub mod pool;
pub mod protocol;
pub mod remote;
pub mod token_store;
pub mod types;

pub use apps::{resolve_app_id, NETFLIX_APP_ID, PRIME_VIDEO_APP_ID, YOUTUBE_APP_ID};
pub use auth::TizenAuthProvider;
pub use client::{
    build_channel_connect_payload, build_channel_launch_payload, build_dial_launch_url,
    build_legacy_launch_payload, build_raw_remote_command_payload, build_remote_command_payload,
    build_ws_url, encode_remote_name, extract_token, extract_tv_name, is_unauthorized,
    parse_channel_connect_response, HandshakeInfo, TizenClient, TizenPairing, TizenSession,
};
pub use discovery::{
    discover as discover_samsung, parse_description, DescriptionIdentity, SamsungTv,
    SAMSUNG_REMOTE_ST,
};
pub use error::{Result, TizenError};
pub use payload::{
    parse_device_info, DeviceInfo, TizenChannelConnectData, TizenChannelConnectParams,
    TizenChannelConnectPayload, TizenChannelConnectResponse, TizenChannelConnectResponseData,
    TizenChannelEmitParams, TizenChannelEmitPayload, TizenChannelLaunchData,
    TizenDialLaunchResponse, TizenLegacyLaunchPayload, TizenRemoteCommand,
    TizenRemoteCommandParams,
};
pub use pool::{
    BoxedChannelFactory, DynConnectionPool, LiveChannelFactory, TizenChannel, TizenChannelFactory,
    TizenConnectionPool,
};

/// Counting test doubles for the connection pool.
///
/// Behind the `testkit` feature so a released build carries no test code: a
/// downstream crate that wants to prove it reuses one channel enables it.
#[cfg(feature = "testkit")]
pub use pool::testkit;
pub use protocol::{DialLauncher, WebSocketHandshake};
pub use remote::{normalize_host, LaunchRoute, LaunchStrategy, PairingOutcome, TizenRemote};
pub use token_store::{load_token, save_token, PairedTv, SamsungTokenStore};
pub use tv_rs_transport::TlsPolicy;
pub use types::{
    validate_remote_key_code, TizenLaunchAction, TizenRemoteKey, TIZEN_APP_ID, TIZEN_CMD_CLICK,
    TIZEN_DEVICE_INFO_PATH, TIZEN_DIAL_DESC_PATH, TIZEN_DIAL_LAUNCH_PREFIX, TIZEN_DIAL_PORT,
    TIZEN_REMOTE_TYPE, TIZEN_REMOTE_WS_PATH, TIZEN_WSS_PORT, TIZEN_WS_PORT,
};
