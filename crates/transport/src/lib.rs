//! `tv-rs-transport` — the transport vocabulary shared by the `tv-rs` protocol crates.
//!
//! The vendor crates (`tv-rs-tizen`, `tv-rs-webos`) drive two different protocols
//! over the *same* transport: a `tokio-tungstenite` WebSocket that may or may not
//! be wrapped in `native-tls`. Everything that is identical across them lives
//! here instead of being copy-pasted, because the copies had to stay
//! byte-identical for security reasons:
//!
//! - [`TlsPolicy`] and [`connector`] — the single home of the
//!   `danger_accept_invalid_*` calls that stand in for the certificate-pinning
//!   bypass of `docs/01-playbook.md` §10 Gap A.
//! - [`read_until`] — the budgeted frame reader that keeps a silent TV from
//!   hanging the caller, replacing four slightly different inline loops.
//! - [`Unsupported`] — the error every crate returns for a deferred
//!   reverse-engineering gap, so no published crate ever calls
//!   `unimplemented!()`.
//!
//! Each protocol crate still works standalone: nothing here knows about Samsung,
//! LG or their payloads.
//!
//! # Example
//!
//! ```
//! use tv_rs_transport::{connector, TlsPolicy, Unsupported};
//!
//! // A plain `ws://` channel is not wrapped in TLS, whatever the policy says.
//! let plain = connector(TlsPolicy::Strict, false).expect("plain connector");
//! assert!(matches!(plain, tokio_tungstenite::Connector::Plain));
//!
//! let gap = Unsupported::new(3, "playbook §10 Gap A");
//! assert_eq!(gap.phase, 3);
//! ```

// The public error enums embed `tokio_tungstenite`'s error, which is large by
// construction. Boxing it would break the #[from] conversions callers rely on,
// and these are cold paths where the extra size costs nothing measurable.
#![forbid(unsafe_code)]
#![allow(clippy::result_large_err)]

#[cfg(feature = "ws")]
pub mod frame;
#[cfg(feature = "ws")]
pub mod retry;
pub mod tls;
pub mod unsupported;

#[cfg(feature = "ws")]
pub use frame::{read_until, ReadOutcome, WsStream};
#[cfg(feature = "ws")]
pub use retry::should_try_next_port;
#[cfg(all(feature = "ws", feature = "rustls"))]
pub use tls::{cert_sink, CertSink};
#[cfg(feature = "ws")]
pub use tls::{connector, TlsError};
pub use tls::{CertFingerprint, TlsPolicy};
pub use unsupported::Unsupported;
