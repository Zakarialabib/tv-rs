//! Data model for a discovered DLNA/UPnP MediaRenderer.
//!
//! `Renderer` is the transport-level device representation (port of the Signage
//! `DlnaDevice`). Field names are kept identical to the app-side device type so
//! the adopting app can map the projection 1:1; the app keeps its own identity
//! (`display_id`) and playlist policy on top of this.

use serde::{Deserialize, Serialize};

/// A discovered DLNA/UPnP MediaRenderer on the LAN.
///
/// A renderer that reports `av_transport_url: None` cannot be cast to (no
/// AVTransport service in its description); one with `av_transport_url` set but
/// `event_sub_url: None` is castable but cannot report state back via GENA —
/// both are legal per UPnP. The same independence holds for
/// `rendering_control_url`: volume control is a separate service, so a
/// castable renderer may still have none.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Renderer {
    /// Unique device name (UDN), e.g. `uuid:2f...-MR-1`.
    pub udn: String,
    /// Human-friendly name shown in UIs (e.g. "Living Room TV").
    pub friendly_name: String,
    /// Manufacturer (Samsung, LG, Sony, ...) or empty when unreported.
    pub manufacturer: String,
    /// Model name/number or empty when unreported.
    pub model_name: String,
    /// IP address of the device.
    pub ip: String,
    /// Location of the device description XML (from the SSDP `LOCATION` header).
    pub location: String,
    /// AVTransport control URL extracted from the device description.
    ///
    /// `None` until the description is fetched (or if the device exposes no
    /// AVTransport service).
    pub av_transport_url: Option<String>,
    /// AVTransport **event** subscription URL (`eventSubURL`) from the same
    /// `<service>` block as `av_transport_url`.
    ///
    /// `None` for a renderer that offers control but no eventing, which is
    /// legal — those devices can be cast to but cannot report state back, so
    /// the caller falls back to knowing only what it commanded.
    #[serde(default)]
    pub event_sub_url: Option<String>,

    /// RenderingControl control URL extracted from the same device description.
    ///
    /// `None` for a renderer that exposes no RenderingControl service — it may be
    /// perfectly castable and still have no volume control to reach.
    #[serde(default)]
    pub rendering_control_url: Option<String>,
}

impl Renderer {
    /// Build a renderer identity from its description fields.
    ///
    /// `av_transport_url` / `event_sub_url` start unset and are filled in by
    /// discovery once the description XML is parsed.
    pub fn new(
        udn: impl Into<String>,
        friendly_name: impl Into<String>,
        manufacturer: impl Into<String>,
        model_name: impl Into<String>,
        ip: impl Into<String>,
        location: impl Into<String>,
    ) -> Self {
        Self {
            udn: udn.into(),
            friendly_name: friendly_name.into(),
            manufacturer: manufacturer.into(),
            model_name: model_name.into(),
            ip: ip.into(),
            location: location.into(),
            av_transport_url: None,
            event_sub_url: None,
            rendering_control_url: None,
        }
    }
}
