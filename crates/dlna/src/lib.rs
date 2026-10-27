//! `tv-rs-dlna` — DLNA/UPnP **transport** for MediaRenderers.
//!
//! This crate owns the renderer-side transport only:
//!
//! - SSDP discovery of MediaRenderers plus device-description parsing
//!   ([`discovery::discover_renderers`], [`discovery::discover_renderers_with_retries`]),
//! - AVTransport SOAP control — `SetAVTransportURI` + `Play` / `Pause` / `Stop` /
//!   `Seek` ([`soap::cast`], [`soap::SoapAvTransport`]),
//! - GENA eventing — `SUBSCRIBE` / renew / `UNSUBSCRIBE` and `LastChange`
//!   NOTIFY parsing ([`gena_client::subscribe`], [`gena_parse::parse_notify_body`])
//!   plus the subscription registry / renewal scheduler
//!   ([`gena_registry::SubscriptionRegistry`]),
//! - SSDP advertisement for a MediaServer the caller hosts
//!   ([`advertise::advertise`], alive/byebye + M-SEARCH responder),
//! - DIDL-Lite metadata generation ([`metadata::build_didl_lite`]).
//!
//! # What this crate is not
//!
//! It is **not** a media server and holds no library: no `MediaItem`, no
//! ContentDirectory, no HTTP file serving, no 206 range responses, no database
//! handle, no content root. [`advertise()`] announces whatever service the caller
//! points `AdvertiseConfig::location` at, [`soap`] pushes whatever URI the
//! caller hands in — whether that URI is backed by a media library is entirely
//! the caller's business (see the Signage adoption spec, fixed boundary §2.2 —
//! the media library and media server stay app-owned there).
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

/// SSDP advertisement for a MediaServer we host (alive/byebye + M-SEARCH responder).
pub mod advertise;
/// SSDP discovery of MediaRenderers + device-description parsing.
pub mod discovery;
/// Error type for DLNA transport operations.
pub mod error;
/// UPnP GENA eventing: request builders, parsers, subscription lifetime.
pub mod gena;
/// GENA HTTP client: SUBSCRIBE / renew / UNSUBSCRIBE.
pub mod gena_client;
/// GENA NOTIFY body parsing (doubly-encoded `LastChange`).
pub mod gena_parse;
/// GENA subscription registry + renewal scheduling (pure bookkeeping).
pub mod gena_registry;
/// DIDL-Lite metadata generation for `SetAVTransportURI`.
pub mod metadata;
/// transport state models (`Renderer`).
pub mod models;
/// Media path selection and optimal delivery routing.
pub mod path;
/// SOAP control for two UPnP services: AVTransport (casting/playback) and
/// RenderingControl (volume/mute).
pub mod soap;
/// Synchronized media delivery and playlist awareness.
pub mod sync;

pub use advertise::{
    advertise, alive_datagrams, byebye_datagrams, parse_msearch_st, search_response,
    should_respond, AdvertiseConfig, AdvertiseHandle, MEDIA_SERVER_DEVICE_TYPE,
};
pub use discovery::{discover_renderers, discover_renderers_with_retries};
pub use error::{DlnaError, Result};
pub use gena::{
    parse_timeout_header, Subscription, TransportEvent, TransportState, AV_TRANSPORT_SERVICE,
    RENEW_MARGIN_FRACTION, REQUESTED_TIMEOUT_SECS,
};
pub use gena_client::{renew, subscribe as subscribe_raw, subscribe_renderer, unsubscribe};
pub use gena_parse::parse_notify_body;
pub use gena_registry::{
    next_tick_delay as next_subscription_tick, plan_action, SubscriptionAction,
    SubscriptionRegistry, TrackedSubscription, MAX_RENEW_FAILURES, MAX_SUBSCRIBE_FAILURES,
};
pub use metadata::build_didl_lite;
pub use models::Renderer;
pub use path::{MediaPath, MediaPathSelector, MediaPathType, SelectionCriteria};
pub use soap::{
    cast, extract_service_control_url, get_mute, get_volume, pause, seek, set_mute, set_volume,
    stop, AvTransport, RenderingControl, SoapAvTransport, SoapRenderingControl,
    AV_TRANSPORT_ACTION, RENDERING_CONTROL_ACTION,
};
pub use sync::{MultiRendererSync, Playlist, PlaylistItem, SyncClock};

#[cfg(test)]
mod gena_registry_tests;
#[cfg(test)]
mod gena_tests;
