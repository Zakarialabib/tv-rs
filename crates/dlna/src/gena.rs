//! UPnP GENA eventing for DLNA renderers.
//!
//! DLNA control is **write-only** without GENA: SOAP `cast` / `pause` / `stop`
//! push commands at the renderer and nothing ever reports back. If someone picks
//! up the TV remote and stops playback, the caller still believes the cast is
//! running. GENA (General Event Notification Architecture) closes the loop: the
//! caller SUBSCRIBEs to the renderer's AVTransport service; the renderer then
//! POSTs a NOTIFY to a callback URL whenever transport state changes.
//!
//! # The two traps this module exists to get right
//!
//! 1. **`LastChange` is doubly-encoded XML** (see [`crate::gena_parse`]).
//! 2. **Subscriptions expire.** The renderer returns a `TIMEOUT: Second-1800`
//!    header and silently stops sending events after it lapses. A subscription
//!    that is never renewed looks exactly like a device that has gone quiet, so
//!    the loop must track expiry and re-subscribe before it happens.
//!
//! # Structure
//!
//! Pure request builders and response parsers live here, free of HTTP — the
//! network calls are thin wrappers in [`crate::gena_client`].

/// Decodes a text event and resolves XML entities.
///
/// `quick-xml` 0.41 split the old `unescape()` into decoding (bytes -> str) and
/// entity resolution (`escape::unescape`). Malformed entities fall back to the empty
/// string, which is what the previous single-call version did.
fn text_of(t: &quick_xml::events::BytesText<'_>) -> String {
    let decoded = t.decode().unwrap_or_default();
    quick_xml::escape::unescape(&decoded)
        .map(|unescaped| unescaped.into_owned())
        .unwrap_or_default()
}

use std::time::{Duration, SystemTime};

use crate::error::DlnaError;

/// AVTransport service type — the service whose events we care about.
pub const AV_TRANSPORT_SERVICE: &str = "urn:schemas-upnp-org:service:AVTransport:1";

/// Requested subscription lifetime. Renderers may grant less; always trust the
/// value in the response rather than this constant.
pub const REQUESTED_TIMEOUT_SECS: u64 = 1800;

/// Renew when this much of the granted lifetime remains.
///
/// A renewal that races the expiry loses the subscription silently, and
/// re-subscribing from scratch means missing every event in between. A fifth of
/// the window (6 min on a 30 min grant) leaves room for a retry or two on a
/// flaky renderer.
pub const RENEW_MARGIN_FRACTION: u32 = 5;

/// Transport state reported by a renderer.
///
/// `Unknown` is deliberate rather than a parse failure: renderers ship
/// vendor-specific states, and an unrecognised value must not discard the rest
/// of an otherwise valid event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportState {
    /// Playback is stopped.
    Stopped,
    /// Playback is running.
    Playing,
    /// Playback is paused (either `PAUSED_PLAYBACK` or `PAUSED`).
    Paused,
    /// The renderer is buffering / switching tracks.
    Transitioning,
    /// No media is loaded.
    NoMedia,
    /// A state value outside the spec (vendor extension).
    Unknown,
}

impl TransportState {
    /// Parse an AVTransport `TransportState` value.
    ///
    /// Case-insensitive: the spec says uppercase, but real devices send
    /// `Playing` and `playing` too.
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_uppercase().as_str() {
            "STOPPED" => Self::Stopped,
            "PLAYING" => Self::Playing,
            "PAUSED_PLAYBACK" | "PAUSED" | "PAUSED_RECORDING" => Self::Paused,
            "TRANSITIONING" => Self::Transitioning,
            "NO_MEDIA_PRESENT" => Self::NoMedia,
            _ => Self::Unknown,
        }
    }

    /// Is the renderer actively showing our content?
    ///
    /// `Transitioning` counts as active: it is the state a renderer passes
    /// through while buffering, and treating it as stopped would make every cast
    /// flicker to "ended" a moment after starting.
    pub fn is_active(self) -> bool {
        matches!(self, Self::Playing | Self::Transitioning)
    }

    /// Canonical string for logs and IPC payloads.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stopped => "stopped",
            Self::Playing => "playing",
            Self::Paused => "paused",
            Self::Transitioning => "transitioning",
            Self::NoMedia => "no_media",
            Self::Unknown => "unknown",
        }
    }
}

/// A parsed transport-state change from a GENA NOTIFY.
#[derive(Debug, Clone, PartialEq)]
pub struct TransportEvent {
    /// New transport state.
    pub state: TransportState,
    /// Current track URI, when the renderer reports one.
    pub current_uri: Option<String>,
    /// `PlaybackStorageMedium`, useful for telling "no disc" from "stopped".
    pub medium: Option<String>,
    /// Last-reported error code. `0` means no error; renderers use non-zero
    /// `TransportErrorCode` / `TransportStatus` values to report a failed load.
    pub error_code: Option<u32>,
}

impl TransportEvent {
    /// Did the renderer report a playback failure?
    ///
    /// Distinguished from a plain stop because a failed load and a user pressing
    /// stop need different UI: one is an error to surface, the other is normal.
    pub fn is_error(&self) -> bool {
        self.error_code.is_some_and(|c| c != 0)
    }
}

/// An active GENA subscription.
#[derive(Debug, Clone)]
pub struct Subscription {
    /// Subscription ID (`SID` header) — required to renew or cancel.
    pub sid: String,
    /// Absolute URL the renderer POSTs NOTIFYs to.
    pub callback_url: String,
    /// The renderer's event endpoint.
    pub event_sub_url: String,
    /// Lifetime granted by the renderer (may be less than requested).
    pub timeout: Duration,
    /// When this subscription was established or last renewed.
    pub established: SystemTime,
}

impl Subscription {
    /// Deadline after which the renderer stops sending events.
    pub fn expires_at(&self) -> SystemTime {
        self.established + self.timeout
    }

    /// Should this be renewed now?
    ///
    /// True once the remaining lifetime drops below
    /// `timeout / RENEW_MARGIN_FRACTION`. Also true when the clock has already
    /// passed the deadline, so a task that was starved (suspend/resume, a blocked
    /// runtime) renews immediately instead of concluding it is still valid.
    pub fn needs_renewal(&self, now: SystemTime) -> bool {
        let margin = self.timeout / RENEW_MARGIN_FRACTION;
        match self.expires_at().duration_since(now) {
            Ok(remaining) => remaining <= margin,
            // `duration_since` errors when `now` is past the deadline.
            Err(_) => true,
        }
    }

    /// Has the subscription definitely lapsed?
    ///
    /// Distinct from [`Subscription::needs_renewal`]: a lapsed subscription
    /// cannot be renewed with its SID and must be re-established from scratch.
    pub fn is_expired(&self, now: SystemTime) -> bool {
        now >= self.expires_at()
    }
}

/// Parse the `TIMEOUT` response header (`Second-1800`, or `infinite`).
///
/// Falls back to the requested value when the header is missing or unparseable —
/// treating an odd header as "no subscription" would drop events from a device
/// that is actually working. `infinite` is clamped to the requested window so
/// the renewal loop still runs: an "infinite" subscription that the device
/// quietly forgets after a reboot would otherwise never be re-established.
pub fn parse_timeout_header(raw: Option<&str>) -> Duration {
    let fallback = Duration::from_secs(REQUESTED_TIMEOUT_SECS);
    let Some(value) = raw else { return fallback };
    let trimmed = value.trim();

    if trimmed.eq_ignore_ascii_case("infinite") {
        return fallback;
    }

    trimmed
        .strip_prefix("Second-")
        .or_else(|| trimmed.strip_prefix("second-"))
        .or_else(|| trimmed.strip_prefix("SECOND-"))
        .and_then(|s| s.trim().parse::<u64>().ok())
        .filter(|s| *s > 0)
        .map(Duration::from_secs)
        .unwrap_or(fallback)
}

/// Build the `CALLBACK` header value for a SUBSCRIBE request.
///
/// GENA requires the angle brackets; a bare URL is rejected by strict renderers
/// with `412 Precondition Failed`.
pub fn callback_header(callback_url: &str) -> String {
    format!("<{callback_url}>")
}

/// Build the `TIMEOUT` header value.
pub fn timeout_header() -> String {
    format!("Second-{REQUESTED_TIMEOUT_SECS}")
}

/// Extract the AVTransport `eventSubURL` from a device description document.
///
/// Mirrors [`crate::soap::extract_service_control_url`] but reads
/// `eventSubURL` instead of `controlURL`. Both live in the same `<service>`
/// block, so a renderer advertising one without the other is malformed — hence
/// the distinct error.
pub fn extract_event_sub_url(description_xml: &str, base_url: &str) -> Result<String, DlnaError> {
    let mut in_service = false;
    let mut service_type = String::new();
    let mut event_url = String::new();
    let mut last_tag = String::new();

    let mut reader = quick_xml::Reader::from_str(description_xml);
    // Text nodes in these documents are indented; without trimming, the `Text`
    // event for whitespace between tags keeps its surrounding newlines and the
    // `serviceType` comparison fails.
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();

    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(quick_xml::events::Event::Start(ref e)) => {
                let name = local_name(e.name().as_ref());
                if name == "service" {
                    in_service = true;
                    service_type.clear();
                    event_url.clear();
                }
                last_tag = name;
            }
            Ok(quick_xml::events::Event::End(ref e)) => {
                if local_name(e.name().as_ref()) == "service" {
                    if service_type.trim() == AV_TRANSPORT_SERVICE && !event_url.is_empty() {
                        return Ok(resolve_relative(base_url, event_url.trim()));
                    }
                    in_service = false;
                }
                // Clear the tag on close so a stray text node after
                // `</eventSubURL>` cannot be attributed to it.
                last_tag.clear();
            }
            Ok(quick_xml::events::Event::Text(ref t)) => {
                if !in_service {
                    continue;
                }
                let text = text_of(t);
                match last_tag.to_ascii_lowercase().as_str() {
                    "servicetype" => service_type = text,
                    "eventsuburl" => event_url = text,
                    _ => {}
                }
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Ok(_) => {}
            Err(e) => return Err(DlnaError::ParseError(e.to_string())),
        }
    }

    Err(DlnaError::ServiceNotFound(AV_TRANSPORT_SERVICE.to_string()))
}

/// Strip a namespace prefix from an element name.
fn local_name(qname: &[u8]) -> String {
    let s = String::from_utf8_lossy(qname);
    s.rsplit(':').next().unwrap_or(&s).to_string()
}

/// Resolve a possibly-relative URL against the description document's URL.
///
/// Shared by the control-URL extractor in [`crate::soap`].
pub(crate) fn resolve_relative(base: &str, path: &str) -> String {
    if path.starts_with("http://") || path.starts_with("https://") {
        return path.to_string();
    }

    // An absolute path replaces the base's path entirely, so it must be joined to
    // the scheme+authority, not to the description's directory.
    if path.starts_with('/') {
        if let Some(scheme_end) = base.find("://") {
            let after = scheme_end + 3;
            let authority_end = base[after..]
                .find('/')
                .map(|p| p + after)
                .unwrap_or(base.len());
            return format!("{}{}", &base[..authority_end], path);
        }
    }

    if let Some(slash) = base.rfind('/') {
        return format!("{}{}", &base[..=slash], path.trim_start_matches('/'));
    }
    format!("{base}/{path}")
}
