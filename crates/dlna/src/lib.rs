//! `tv-rs-dlna` — DLNA/UPnP **transport** for MediaRenderers.
//!
//! This crate owns the renderer-side transport only:
//!
//! - SSDP discovery of MediaRenderers plus device-description parsing
//!   ([`discovery::discover_renderers`], [`discovery::discover_renderers_with_retries`]),
//! - AVTransport SOAP control — `SetAVTransportURI` + `Play` / `Pause` / `Stop` /
//!   `Seek` ([`soap::cast`], [`soap::SoapAvTransport`]),
//! - GENA eventing — `SUBSCRIBE` / renew / `UNSUBSCRIBE` and `LastChange`
//!   NOTIFY parsing ([`gena_client::subscribe`], [`gena_parse::parse_notify_body`]),
//! - DIDL-Lite metadata generation ([`metadata::build_didl_lite`]).
//!
//! # What this crate is not
//!
//! It is **not** a media server and holds no library: no `MediaItem`, no
//! ContentDirectory, no HTTP file serving, no 206 range responses, no database
//! handle, no content root. Callers hand in a URI (and optional DIDL metadata);
//! whether that URI is backed by a library is entirely the caller's business
//! (see the Signage adoption spec, fixed boundary §2.2 — the media library and
//! media server stay app-owned there).
//!
//! # Example
//!
//! ```no_run
//! use std::time::Duration;
//! use tv_rs_dlna::{discover_renderers, soap, Renderer};
//!
//! # async fn run() -> Result<(), tv_rs_dlna::DlnaError> {
//! let renderers: Vec<Renderer> = discover_renderers(Duration::from_secs(3)).await?;
//! let renderer = renderers.first().expect("at least one renderer");
//! let control = renderer.av_transport_url.as_deref().expect("castable renderer");
//!
//! soap::cast(control, "http://media.example/stream.m3u8", "Lobby show").await?;
//! # Ok(())
//! # }
//! ```

#![forbid(unsafe_code)]

/// Error type for DLNA transport operations.
pub mod error;
/// SSDP discovery of MediaRenderers + device-description parsing.
pub mod discovery;
/// transport state models (`Renderer`).
pub mod models;
/// DIDL-Lite metadata generation for `SetAVTransportURI`.
pub mod metadata;
/// AVTransport SOAP control (`SetAVTransportURI` / `Play` / `Pause` / `Stop` / `Seek`).
pub mod soap;
/// UPnP GENA eventing: request builders, parsers, subscription lifetime.
pub mod gena;
/// GENA HTTP client: SUBSCRIBE / renew / UNSUBSCRIBE.
pub mod gena_client;
/// GENA NOTIFY body parsing (doubly-encoded `LastChange`).
pub mod gena_parse;

pub use discovery::{discover_renderers, discover_renderers_with_retries};
pub use error::{DlnaError, Result};
pub use gena::{
    parse_timeout_header, Subscription, TransportEvent, TransportState, AV_TRANSPORT_SERVICE,
    REQUESTED_TIMEOUT_SECS, RENEW_MARGIN_FRACTION,
};
pub use gena_client::{renew, subscribe_renderer, unsubscribe, subscribe as subscribe_raw};
pub use gena_parse::parse_notify_body;
pub use metadata::build_didl_lite;
pub use models::Renderer;
pub use soap::{cast, extract_av_transport_control_url, pause, seek, stop, AvTransport, SoapAvTransport};

#[cfg(test)]
mod gena_tests;
