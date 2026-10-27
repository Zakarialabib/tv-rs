//! `tv-rs` — cross-platform TV control and communications facade.
//!
//! One dependency that finds televisions on the local network and drives them,
//! regardless of vendor. The protocol work lives in five focused crates, which
//! this facade re-exports and coordinates:
//!
//! | Crate | Responsibility |
//! | --- | --- |
//! | [`discovery`] | SSDP `M-SEARCH` and UPnP descriptor parsing |
//! | [`power`] | Wake-on-LAN magic packets and power-state probing |
//! | [`tizen`] | Samsung Tizen remote channel and DIAL |
//! | [`webos`] | LG webOS SSAP over WebSocket |
//! | [`roku`] | Roku ECP REST |
//!
//! Discovery needs no credentials — SSDP is an unauthenticated LAN multicast
//! protocol. Control on Roku also needs none; Tizen and webOS prompt on the TV
//! the first time and issue a token/`client-key` to reuse afterwards.
//!
//! # Example
//!
//! ```no_run
//! use tv_rs::{discover, TvVendor};
//!
//! # async fn run() -> tv_rs::Result<()> {
//! for tv in discover().await? {
//!     println!("{} ({}) at {}", tv.label(), tv.vendor.as_str(), tv.ip);
//!
//!     if tv.vendor != TvVendor::Unknown {
//!         let client = tv.to_client()?;
//!         client.power_off().await?;
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! The facade deliberately covers only the operations that have an equivalent on
//! every vendor. Anything else — webOS volume control, Roku app listing, Tizen
//! raw frames — is reached through the re-exported protocol crates or the typed
//! accessors on [`TvClient`].

// The core `TvError` carries pre-vendor payloads (transport, protocol, auth),
// which are small; vendor errors are converted, never embedded, so no `Err`
// variant is large and no boxing shows up in the facade.
#![forbid(unsafe_code)]

mod client;
mod error;
mod handle;
mod types;

pub use client::{
    discover, discover_stream, discover_with, identify_vendor, mdc_handle, probe, sicp_handle,
    TvClient,
};
pub use error::{Result, TvError};
pub use types::{DiscoveredTv, TvVendor};

/// The one home of vendor-error → [`TvError`] mapping.
///
/// Callers that use a protocol crate directly (as the CLI does) map through
/// these functions, so the remedy a vendor error carries is decided **once**,
/// in one place, with no wildcard arm to hide a new variant. This is the module
/// the A2 gap describes: adding a `TizenError`, `WebosError` or `RokuError`
/// variant is a compile error here until someone names its remedy.
pub mod map;

/// SSDP & mDNS discovery, UPnP descriptor parsing, and signage pairing: [`discovery::SsdpScanner`], [`discovery::MdnsScanner`].
pub use tv_rs_cec as cec;
pub use tv_rs_core as core;
pub use tv_rs_discovery as discovery;
pub use tv_rs_ir as ir;
/// Samsung MDC commercial display protocol client: [`mdc::MdcClient`].
pub use tv_rs_mdc as mdc;
/// Wake-on-LAN and power-state probing: [`power::WolSender`].
pub use tv_rs_power as power;
pub use tv_rs_power::PowerState;
pub use tv_rs_roku as roku;
/// Philips SICP commercial display protocol client: [`sicp::SicpClient`].
pub use tv_rs_sicp as sicp;
/// Samsung Tizen remote channel and DIAL: [`tizen::TizenClient`].
pub use tv_rs_tizen as tizen;
/// TLS policy and pinning, shared by every vendor channel: [`transport::TlsPolicy`].
pub use tv_rs_transport as transport;
/// LG webOS SSAP WebSocket client: [`webos::WebosClient`].
pub use tv_rs_webos as webos;
