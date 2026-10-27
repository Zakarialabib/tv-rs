//! SOAP control for DLNA/UPnP MediaRenderers.
//!
//! Given a control URL (extracted from the device description during discovery),
//! this module sends the actions of two UPnP services, which a renderer may expose
//! independently of each other:
//!
//! - **AVTransport** (`SetAVTransportURI` / `Play` / `Pause` / `Stop` / `Seek` /
//!   `GetTransportInfo` / `GetPositionInfo`) — casting and playback.
//! - **RenderingControl** (`GetVolume` / `SetVolume` / `GetMute` / `SetMute`) —
//!   volume and mute.
//!
//! A renderer can cast without being able to report volume, so discovery records
//! both control URLs and each is optional.
//!
//! Two flavours are offered for each service:
//!
//! - free functions ([`cast`], [`stop`], [`pause`], [`seek`], [`get_volume`]) for
//!   one-shot use,
//! - [`SoapAvTransport`] / [`SoapRenderingControl`] implementing the [`AvTransport`]
//!   / [`RenderingControl`] traits, for callers that hold a renderer handle and
//!   want a typed control surface.

/// Resolves XML entities in a text event.
///
/// `quick-xml` 0.42 stores text events as already-decoded `str` with entity
/// references left as-is, so only entity resolution (`escape::unescape`) remains
/// here. Malformed entities fall back to the empty string, which is what the
/// previous single-call version did.
fn text_of(t: &quick_xml::events::BytesText<'_>) -> String {
    quick_xml::escape::unescape(t.as_ref())
        .map(|unescaped| unescaped.into_owned())
        .unwrap_or_default()
}

use std::time::Duration;

use async_trait::async_trait;

use crate::error::DlnaError;
use crate::metadata::escape_xml;

/// AVTransport service type used for the `SOAPACTION` header.
pub const AV_TRANSPORT_ACTION: &str = "urn:schemas-upnp-org:service:AVTransport:1";

/// RenderingControl service type used for the `SOAPACTION` header.
///
/// A separate service with its own control URL, which a renderer may expose
/// without AVTransport.
pub const RENDERING_CONTROL_ACTION: &str = "urn:schemas-upnp-org:service:RenderingControl:1";

/// Product token advertised to renderers.
///
/// `DLNADOC/1.50` matters: some renderers gate behaviour on seeing a
/// DLNA-document version in the user agent.
const USER_AGENT: &str = "tv-rs-dlna/0.1.0 DLNADOC/1.50";

/// Find one service's `controlURL` within a device description XML.
///
/// `service_urn` is the UPnP service type to match — [`AV_TRANSPORT_ACTION`] or
/// [`RENDERING_CONTROL_ACTION`] — so the same walk serves both services instead of
/// duplicating the XML handling for each.
///
/// Returns the absolute control URL (resolved against the description base if
/// the `controlURL` is relative, which UPnP permits).
///
/// # Errors
///
/// Returns [`DlnaError::ServiceNotFound`] when the description carries no service
/// of that type, naming the type so a caller can tell "not castable" from "no
/// volume control" apart.
pub fn extract_service_control_url(
    description_xml: &str,
    base_url: &str,
    service_urn: &str,
) -> Result<String, DlnaError> {
    let mut reader = quick_xml::Reader::from_str(description_xml);
    // Text nodes in these documents are usually indented; without trimming, the
    // captured `serviceType` keeps its surrounding whitespace and the comparison
    // against the AVTransport type fails.
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();

    let mut in_service = false;
    let mut service_type = String::new();
    let mut control_url = String::new();
    let mut last_tag = String::new();

    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(quick_xml::events::Event::Start(ref e)) => {
                let name = local_name(e.name().as_ref());
                if name == "service" {
                    in_service = true;
                    service_type.clear();
                    control_url.clear();
                }
                last_tag = name;
            }
            Ok(quick_xml::events::Event::End(ref e)) => {
                if local_name(e.name().as_ref()) == "service" {
                    if service_type.trim() == service_urn && !control_url.is_empty() {
                        return Ok(resolve_url(base_url, &control_url));
                    }
                    in_service = false;
                }
                // Clear the tag on close so a stray text node after `</controlURL>`
                // cannot be attributed to it.
                last_tag.clear();
            }
            Ok(quick_xml::events::Event::Text(ref t)) => {
                if !in_service {
                    continue;
                }
                let text = text_of(t);
                match last_tag.to_ascii_lowercase().as_str() {
                    "servicetype" => service_type = text,
                    "controlurl" => control_url = text,
                    _ => {}
                }
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Ok(_) => {}
            Err(e) => return Err(DlnaError::ParseError(e.to_string())),
        }
    }

    Err(DlnaError::ServiceNotFound(service_urn.to_string()))
}

/// Strip a namespace prefix from an element name.
///
/// `quick-xml` 0.42 hands out names as `&str` (`QName` wraps `str`, not bytes).
fn local_name(qname: &str) -> String {
    qname.rsplit(':').next().unwrap_or(qname).to_string()
}

/// Resolve a control URL against the description document's URL.
///
/// Shares the semantics of the event-URL resolver: a `http`/`https`-absolute URL
/// is returned unchanged, a path starting with `/` is joined to the base's
/// scheme+authority (not the description's directory), and everything else is
/// joined onto the base directory.
fn resolve_url(base: &str, control: &str) -> String {
    crate::gena::resolve_relative(base, control.trim())
}

/// Send a SOAP action to a renderer's AVTransport control URL.
/// Posts one SOAP action and returns the response body.
///
/// Split from [`send_soap`] because the queries need the body and the commands
/// throw it away — and a renderer puts its answer in that body, not in the status.
async fn post_soap(
    control_url: &str,
    service_urn: &str,
    action: &str,
    body: &str,
) -> Result<String, DlnaError> {
    let envelope = format!(
        r#"<?xml version="1.0" encoding="utf-8"?><s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/"><s:Body>{body}</s:Body></s:Envelope>"#
    );
    let soap_action = format!("\"{service_urn}#{action}\"");

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| DlnaError::SoapError(e.to_string()))?;

    let resp = client
        .post(control_url)
        .header("Content-Type", "text/xml; charset=\"utf-8\"")
        .header("SOAPACTION", soap_action)
        .header("USER-AGENT", USER_AGENT)
        .body(envelope)
        .send()
        .await
        .map_err(|e| DlnaError::SoapError(e.to_string()))?;

    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();

    if !status.is_success() {
        // Renderers often return 200/500 with a fault body; surface the body.
        if text.contains("errorCode") || text.contains("fault") {
            return Err(DlnaError::AvTransportFault(text));
        }
        return Err(DlnaError::SoapError(format!("HTTP {status}")));
    }
    Ok(text)
}

async fn send_soap(
    control_url: &str,
    service_urn: &str,
    action: &str,
    body: &str,
) -> Result<(), DlnaError> {
    post_soap(control_url, service_urn, action, body)
        .await
        .map(|_| ())
}

/// `SetAVTransportURI` + `Play` — push `media_url` to the renderer and start it.
pub async fn cast(control_url: &str, media_url: &str, title: &str) -> Result<(), DlnaError> {
    let didl = crate::metadata::build_didl_lite(title, media_url);
    set_uri(control_url, media_url, &didl).await?;
    play(control_url).await
}

/// `SetAVTransportURI` without an implicit `Play`.
///
/// Lets a caller push a URI and control playback start separately (some
/// renderers buffer for a long time and it is nicer to send `Play` at exactly
/// the right beat).
pub async fn set_uri(control_url: &str, media_url: &str, didl: &str) -> Result<(), DlnaError> {
    let set_body = format!(
        "<u:SetAVTransportURI xmlns:u=\"{AV_TRANSPORT_ACTION}\"><InstanceID>0</InstanceID><CurrentURI>{uri}</CurrentURI><CurrentURIMetaData>{meta}</CurrentURIMetaData></u:SetAVTransportURI>",
        uri = escape_xml(media_url),
        meta = escape_xml(didl),
    );
    send_soap(
        control_url,
        AV_TRANSPORT_ACTION,
        "SetAVTransportURI",
        &set_body,
    )
    .await
}

/// `Play` the currently-set URI at normal speed.
pub async fn play(control_url: &str) -> Result<(), DlnaError> {
    let play_body = format!(
        "<u:Play xmlns:u=\"{AV_TRANSPORT_ACTION}\"><InstanceID>0</InstanceID><Speed>1</Speed></u:Play>",
    );
    send_soap(control_url, AV_TRANSPORT_ACTION, "Play", &play_body).await
}

/// `Stop` the current playback on the renderer.
pub async fn stop(control_url: &str) -> Result<(), DlnaError> {
    let stop_body =
        format!("<u:Stop xmlns:u=\"{AV_TRANSPORT_ACTION}\"><InstanceID>0</InstanceID></u:Stop>",);
    send_soap(control_url, AV_TRANSPORT_ACTION, "Stop", &stop_body).await
}

/// `Pause` the current playback.
pub async fn pause(control_url: &str) -> Result<(), DlnaError> {
    let pause_body =
        format!("<u:Pause xmlns:u=\"{AV_TRANSPORT_ACTION}\"><InstanceID>0</InstanceID></u:Pause>",);
    send_soap(control_url, AV_TRANSPORT_ACTION, "Pause", &pause_body).await
}

/// `Seek` to a position within the current track, from its start.
///
/// Uses `REL_TIME`; `position` is bounded to whole seconds because AVTransport
/// timestamp precision beyond that is not honoured by real devices.
pub async fn seek(control_url: &str, position: Duration) -> Result<(), DlnaError> {
    let h = position.as_secs() / 3600;
    let m = (position.as_secs() % 3600) / 60;
    let s = position.as_secs() % 60;
    let target = format!("{h:02}:{m:02}:{s:02}");
    let seek_body = format!(
        "<u:Seek xmlns:u=\"{AV_TRANSPORT_ACTION}\"><InstanceID>0</InstanceID><Unit>REL_TIME</Unit><Target>{target}</Target></u:Seek>",
    );
    send_soap(control_url, AV_TRANSPORT_ACTION, "Seek", &seek_body).await
}

/// What a renderer reports about its transport right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportInfo {
    /// `PLAYING`, `PAUSED_PLAYBACK`, `STOPPED`, `TRANSITIONING`, `NO_MEDIA_PRESENT`.
    pub state: String,
    /// `OK` on a healthy renderer; anything else is worth surfacing.
    pub status: Option<String>,
    /// Playback rate, `1` at normal speed.
    pub speed: Option<String>,
}

/// Where a renderer is inside its current track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PositionInfo {
    /// Position from the start of the track.
    pub rel_time: Duration,
    /// Track length, when the renderer reports a real one.
    ///
    /// `None` covers both "not implemented" and the all-zeroes some renderers send
    /// for a live stream — neither is a length of zero.
    pub track_duration: Option<Duration>,
    /// The URI being played, when the renderer reports it.
    pub track_uri: Option<String>,
}

/// `GetTransportInfo`: the state a cast is actually in, rather than an estimate.
pub async fn get_transport_info(control_url: &str) -> Result<TransportInfo, DlnaError> {
    let body = format!(
        "<u:GetTransportInfo xmlns:u=\"{AV_TRANSPORT_ACTION}\"><InstanceID>0</InstanceID></u:GetTransportInfo>",
    );
    let response = post_soap(control_url, AV_TRANSPORT_ACTION, "GetTransportInfo", &body).await?;
    parse_transport_info(&response)
}

/// `GetPositionInfo`: how far into the track a renderer is.
pub async fn get_position_info(control_url: &str) -> Result<PositionInfo, DlnaError> {
    let body = format!(
        "<u:GetPositionInfo xmlns:u=\"{AV_TRANSPORT_ACTION}\"><InstanceID>0</InstanceID></u:GetPositionInfo>",
    );
    let response = post_soap(control_url, AV_TRANSPORT_ACTION, "GetPositionInfo", &body).await?;
    parse_position_info(&response)
}

/// Parses a `GetTransportInfo` response.
///
/// Split from the request so the shape is testable without a renderer: the state
/// is the one field that must be there, and a missing one is an error rather than
/// an empty string that reads like `STOPPED`.
fn parse_transport_info(xml: &str) -> Result<TransportInfo, DlnaError> {
    let state = field(xml, "CurrentTransportState").ok_or_else(|| {
        DlnaError::SoapError("GetTransportInfo carried no CurrentTransportState".to_string())
    })?;
    Ok(TransportInfo {
        state,
        status: field(xml, "CurrentTransportStatus"),
        speed: field(xml, "CurrentSpeed"),
    })
}

/// Parses a `GetPositionInfo` response.
fn parse_position_info(xml: &str) -> Result<PositionInfo, DlnaError> {
    let rel_time = field(xml, "RelTime")
        .as_deref()
        .and_then(parse_hms)
        .unwrap_or_default();
    Ok(PositionInfo {
        rel_time,
        track_duration: field(xml, "TrackDuration")
            .as_deref()
            .and_then(parse_hms)
            .filter(|duration| !duration.is_zero()),
        track_uri: field(xml, "TrackURI").filter(|uri| !uri.is_empty()),
    })
}

/// Parses a DLNA `H+:MM:SS` timestamp.
///
/// Renderers answer `NOT_IMPLEMENTED` and `00:00:00` for "unknown", so `None` means
/// "no answer" and a caller must not read it as zero.
fn parse_hms(value: &str) -> Option<Duration> {
    let mut parts = value.trim().split(':');
    let hours: u64 = parts.next()?.parse().ok()?;
    let minutes: u64 = parts.next()?.parse().ok()?;
    let seconds: u64 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(Duration::from_secs(hours * 3600 + minutes * 60 + seconds))
}

/// The text of the first `<tag>…</tag>` in `xml`.
///
/// This crate has no XML dependency on purpose: what it reads is a flat list of
/// known tags, and a parser would be more machinery than the job needs.
fn field(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    Some(unescape_xml(xml[start..end].trim()))
}

/// Reverses the five predefined XML entities. `&amp;` goes last, so a literal
/// `&amp;lt;` is not turned into `<`.
fn unescape_xml(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// Typed transport-control surface for one renderer.
///
/// The crate-primary way to drive playback after discovery.
#[async_trait]
pub trait AvTransport: Send + Sync {
    /// Push a URI with optional DIDL metadata (no implicit play).
    async fn set_uri(&self, uri: &str, didl: &str) -> Result<(), DlnaError>;
    /// Start or resume playback.
    async fn play(&self) -> Result<(), DlnaError>;
    /// Pause playback.
    async fn pause(&self) -> Result<(), DlnaError>;
    /// Stop playback.
    async fn stop(&self) -> Result<(), DlnaError>;
    /// Seek to a position from the start of the track.
    async fn seek(&self, position: Duration) -> Result<(), DlnaError>;
    /// Read the transport state — a cold read, not an estimate.
    async fn transport_info(&self) -> Result<TransportInfo, DlnaError>;
    /// Read the position within the current track.
    async fn position_info(&self) -> Result<PositionInfo, DlnaError>;
}

/// [`AvTransport`] implementation that talks SOAP to one control URL.
#[derive(Debug, Clone)]
pub struct SoapAvTransport {
    /// The AVTransport control endpoint of the target renderer.
    control_url: String,
}

impl SoapAvTransport {
    /// Create a control handle for the given AVTransport control URL.
    pub fn new(control_url: impl Into<String>) -> Self {
        Self {
            control_url: control_url.into(),
        }
    }

    /// The underlying control URL.
    pub fn control_url(&self) -> &str {
        &self.control_url
    }
}

#[async_trait]
impl AvTransport for SoapAvTransport {
    async fn set_uri(&self, uri: &str, didl: &str) -> Result<(), DlnaError> {
        set_uri(&self.control_url, uri, didl).await
    }

    async fn play(&self) -> Result<(), DlnaError> {
        play(&self.control_url).await
    }

    async fn pause(&self) -> Result<(), DlnaError> {
        pause(&self.control_url).await
    }

    async fn stop(&self) -> Result<(), DlnaError> {
        stop(&self.control_url).await
    }

    async fn seek(&self, position: Duration) -> Result<(), DlnaError> {
        seek(&self.control_url, position).await
    }

    async fn transport_info(&self) -> Result<TransportInfo, DlnaError> {
        get_transport_info(&self.control_url).await
    }

    async fn position_info(&self) -> Result<PositionInfo, DlnaError> {
        get_position_info(&self.control_url).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TRANSPORT_INFO: &str = r#"<?xml version="1.0"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/"><s:Body>
<u:GetTransportInfoResponse xmlns:u="urn:schemas-upnp-org:service:AVTransport:1">
<CurrentTransportState>PLAYING</CurrentTransportState>
<CurrentTransportStatus>OK</CurrentTransportStatus>
<CurrentSpeed>1</CurrentSpeed>
</u:GetTransportInfoResponse></s:Body></s:Envelope>"#;

    const POSITION_INFO: &str = r#"<?xml version="1.0"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/"><s:Body>
<u:GetPositionInfoResponse xmlns:u="urn:schemas-upnp-org:service:AVTransport:1">
<Track>1</Track>
<TrackDuration>00:03:25</TrackDuration>
<TrackURI>http://192.168.1.5:9000/a%20b.mp4</TrackURI>
<RelTime>00:00:42</RelTime>
</u:GetPositionInfoResponse></s:Body></s:Envelope>"#;

    #[test]
    fn transport_info_parses_the_state_a_cast_is_actually_in() {
        let info = parse_transport_info(TRANSPORT_INFO).expect("fixture parses");
        assert_eq!(info.state, "PLAYING");
        assert_eq!(info.status.as_deref(), Some("OK"));
        assert_eq!(info.speed.as_deref(), Some("1"));
    }

    #[test]
    fn transport_info_without_a_state_is_an_error_not_an_empty_string() {
        // An empty state would read like `STOPPED` to a caller.
        assert!(parse_transport_info("<u:GetTransportInfoResponse/>").is_err());
    }

    #[test]
    fn position_info_parses_the_clock_and_the_track() {
        let info = parse_position_info(POSITION_INFO).expect("fixture parses");
        assert_eq!(info.rel_time, Duration::from_secs(42));
        assert_eq!(info.track_duration, Some(Duration::from_secs(205)));
        assert_eq!(
            info.track_uri.as_deref(),
            Some("http://192.168.1.5:9000/a%20b.mp4")
        );
    }

    #[test]
    fn an_unknown_duration_is_none_rather_than_zero() {
        // Renderers answer `NOT_IMPLEMENTED` for a live stream, and some send
        // all-zeroes. Neither is a length of zero seconds.
        let xml = "<u:GetPositionInfoResponse><TrackDuration>NOT_IMPLEMENTED</TrackDuration>\
                   <RelTime>00:00:07</RelTime></u:GetPositionInfoResponse>";
        let info = parse_position_info(xml).expect("fixture parses");
        assert_eq!(info.track_duration, None);
        assert_eq!(info.rel_time, Duration::from_secs(7));
    }

    #[test]
    fn hms_handles_hours_and_refuses_nonsense() {
        assert_eq!(parse_hms("01:02:03"), Some(Duration::from_secs(3723)));
        assert_eq!(parse_hms("00:00:00"), Some(Duration::ZERO));
        assert_eq!(parse_hms("NOT_IMPLEMENTED"), None);
        assert_eq!(parse_hms("00:01"), None);
    }

    const DESC: &str = r#"<root xmlns="urn:schemas-upnp-org:device-1-0"><device>
    <serviceList>
        <service>
        <serviceType>urn:schemas-upnp-org:service:RenderingControl:1</serviceType>
        <controlURL>/RenderingControl/ctrl</controlURL>
        </service>
        <service>
        <serviceType>urn:schemas-upnp-org:service:AVTransport:1</serviceType>
        <controlURL>/AVTransport/ctrl</controlURL>
        </service>
    </serviceList>
    </device></root>"#;

    #[test]
    fn picks_the_avtransport_control_url_not_another_service() {
        // RenderingControl comes first and has its own controlURL; a parser that
        // takes the first match would send volume commands instead of casts.
        let url =
            extract_service_control_url(DESC, "http://10.0.0.5:8080/desc.xml", AV_TRANSPORT_ACTION)
                .unwrap();
        assert_eq!(url, "http://10.0.0.5:8080/AVTransport/ctrl");
    }

    #[test]
    fn a_relative_control_path_is_joined_to_the_authority_not_the_directory() {
        let url = extract_service_control_url(
            DESC,
            "http://10.0.0.5:8080/dmr/desc.xml",
            AV_TRANSPORT_ACTION,
        )
        .unwrap();
        assert_eq!(url, "http://10.0.0.5:8080/AVTransport/ctrl");
    }

    #[test]
    fn an_already_absolute_control_url_is_left_alone() {
        let desc = r#"<root><service>
        <serviceType>urn:schemas-upnp-org:service:AVTransport:1</serviceType>
        <controlURL>http://other.host:1400/ctrl</controlURL>
        </service></root>"#;
        let url =
            extract_service_control_url(desc, "http://10.0.0.5:8080/desc.xml", AV_TRANSPORT_ACTION)
                .unwrap();
        assert_eq!(url, "http://other.host:1400/ctrl");
    }

    #[test]
    fn a_renderer_without_avtransport_is_an_error() {
        let desc = r#"<root><service>
        <serviceType>urn:schemas-upnp-org:service:RenderingControl:1</serviceType>
        <controlURL>/rc/ctrl</controlURL>
        </service></root>"#;
        assert!(
            extract_service_control_url(desc, "http://h/desc.xml", AV_TRANSPORT_ACTION).is_err()
        );
    }

    #[test]
    fn namespaced_elements_are_parsed() {
        // Some vendors prefix service children (e:serviceType); local-name matching
        // handles both spellings.
        let desc = r#"<root xmlns:e="urn:schemas-upnp-org:device-1-0"><e:service>
        <e:serviceType>urn:schemas-upnp-org:service:AVTransport:1</e:serviceType>
        <e:controlURL>/AVTransport/ctrl</e:controlURL>
        </e:service></root>"#;
        let url =
            extract_service_control_url(desc, "http://h/desc.xml", AV_TRANSPORT_ACTION).unwrap();
        assert_eq!(url, "http://h/AVTransport/ctrl");
    }

    #[test]
    fn seek_timestamps_are_zero_padded() {
        // 75s must become "00:01:15", not "0:1:15" (which renderers reject).
        let format_position = |secs: u64| -> String {
            let h = secs / 3600;
            let m = (secs % 3600) / 60;
            let s = secs % 60;
            format!("{h:02}:{m:02}:{s:02}")
        };
        assert_eq!(format_position(75), "00:01:15");
        assert_eq!(format_position(3661), "01:01:01");
    }

    #[tokio::test]
    async fn soap_av_transport_routes_calls_to_the_control_url() {
        // Smoke-test the trait client against a unreachable URL: it must produce a
        // SoapError (network) rather than panic — proves routing, not the wire.
        let client = SoapAvTransport::new("http://127.0.0.1:1/ctrl");
        let result = client.stop().await;
        assert!(matches!(result, Err(DlnaError::SoapError(_))));
        assert_eq!(client.control_url(), "http://127.0.0.1:1/ctrl");
    }
}

// ---------------------------------------------------------------------------
// RenderingControl — volume and mute. A separate UPnP service with its own
// control URL, which a renderer may expose without AVTransport.
// ---------------------------------------------------------------------------

/// `GetVolume` — the renderer's current volume, 0..=100.
///
/// The UPnP `Volume` state variable is `ui2` with a declared range of 0..100, so
/// the answer is parsed as a `u8` and a value outside that range is a protocol
/// fault rather than a volume. A renderer that reports `0` is silent, not broken.
pub async fn get_volume(control_url: &str) -> Result<u8, DlnaError> {
    let body = format!(
        "<u:GetVolume xmlns:u=\"{RENDERING_CONTROL_ACTION}\"><InstanceID>0</InstanceID><Channel>Master</Channel></u:GetVolume>",
    );
    let response = post_soap(control_url, RENDERING_CONTROL_ACTION, "GetVolume", &body).await?;
    parse_volume(&response)
}

/// Parses a `GetVolume` response body into 0..=100.
///
/// Split from the request so the shape is testable without a renderer. The UPnP
/// `Volume` variable is `ui2` with a declared range of 0..100, so a value outside
/// that is a protocol fault rather than a volume: a renderer that reports `0` is
/// silent, not broken, and one that reports `101` is lying.
fn parse_volume(xml: &str) -> Result<u8, DlnaError> {
    let raw = field(xml, "CurrentVolume")
        .ok_or_else(|| DlnaError::SoapError("GetVolume carried no CurrentVolume".to_string()))?;
    let trimmed = raw.trim();
    let volume = trimmed.parse::<u8>().map_err(|e| {
        DlnaError::SoapError(format!("CurrentVolume is not a number: {trimmed:?} ({e})"))
    })?;
    // `u8` holds 0..=255, so the cast alone would happily accept 101..=255. The
    // declared range is 0..=100, so the bound has to be checked explicitly: a
    // renderer reporting 101 is lying, not loud, and `try_from` cannot tell the two
    // apart.
    if volume > 100 {
        return Err(DlnaError::SoapError(format!(
            "CurrentVolume {volume} is outside the declared 0..=100 range"
        )));
    }
    Ok(volume)
}

/// `SetVolume` — set the renderer's volume, 0..=100.
///
/// A renderer clamps rather than faults, so the value it settles on is not
/// necessarily the one asked for; read it back with [`get_volume`] if the exact
/// level matters.
pub async fn set_volume(control_url: &str, volume: u8) -> Result<(), DlnaError> {
    let body = format!(
        "<u:SetVolume xmlns:u=\"{RENDERING_CONTROL_ACTION}\"><InstanceID>0</InstanceID><Channel>Master</Channel><DesiredVolume>{volume}</DesiredVolume></u:SetVolume>",
    );
    send_soap(control_url, RENDERING_CONTROL_ACTION, "SetVolume", &body).await
}

/// `GetMute` — whether the renderer is muted.
///
/// UPnP encodes a boolean as `0`/`1`, but a handful of renderers answer
/// `true`/`false`, so both spellings are accepted rather than one being a fault.
pub async fn get_mute(control_url: &str) -> Result<bool, DlnaError> {
    let body = format!(
        "<u:GetMute xmlns:u=\"{RENDERING_CONTROL_ACTION}\"><InstanceID>0</InstanceID><Channel>Master</Channel></u:GetMute>",
    );
    let response = post_soap(control_url, RENDERING_CONTROL_ACTION, "GetMute", &body).await?;
    parse_mute(&response)
}

/// Parses a `GetMute` response body.
///
/// UPnP encodes a boolean as `0`/`1`, but a handful of renderers answer
/// `true`/`false`, so both spellings are accepted rather than one being a fault.
/// `0` is muted-off rather than an error: a muted renderer is a real state, not a
/// failure.
fn parse_mute(xml: &str) -> Result<bool, DlnaError> {
    let raw = field(xml, "CurrentMute")
        .ok_or_else(|| DlnaError::SoapError("GetMute carried no CurrentMute".to_string()))?;
    Ok(matches!(raw.trim(), "1" | "true" | "TRUE" | "True"))
}

/// `SetMute` — mute or unmute the renderer.
///
/// Sends `1`/`0`, the `ui2`-style encoding the UPnP boolean datatype uses in SOAP.
pub async fn set_mute(control_url: &str, muted: bool) -> Result<(), DlnaError> {
    let desired = u8::from(muted);
    let body = format!(
        "<u:SetMute xmlns:u=\"{RENDERING_CONTROL_ACTION}\"><InstanceID>0</InstanceID><Channel>Master</Channel><DesiredMute>{desired}</DesiredMute></u:SetMute>",
    );
    send_soap(control_url, RENDERING_CONTROL_ACTION, "SetMute", &body).await
}

/// Volume and mute control for a renderer's `RenderingControl` service.
///
/// Separate from [`AvTransport`] because it is a separate UPnP service: a
/// renderer can offer playback control without exposing volume, and discovery
/// records which of the two it has.
#[async_trait]
pub trait RenderingControl: Send + Sync {
    /// Current volume, 0..=100.
    async fn volume(&self) -> Result<u8, DlnaError>;

    /// Set the volume. The renderer clamps rather than faults.
    async fn set_volume(&self, volume: u8) -> Result<(), DlnaError>;

    /// Whether the renderer is muted.
    async fn muted(&self) -> Result<bool, DlnaError>;

    /// Mute or unmute.
    async fn set_muted(&self, muted: bool) -> Result<(), DlnaError>;
}

/// [`RenderingControl`] implementation that talks SOAP to one control URL.
#[derive(Debug, Clone)]
pub struct SoapRenderingControl {
    /// The RenderingControl control endpoint of the target renderer.
    control_url: String,
}

impl SoapRenderingControl {
    /// Create a control handle for the given RenderingControl control URL.
    pub fn new(control_url: impl Into<String>) -> Self {
        Self {
            control_url: control_url.into(),
        }
    }

    /// The underlying control URL.
    pub fn control_url(&self) -> &str {
        &self.control_url
    }
}

#[async_trait]
impl RenderingControl for SoapRenderingControl {
    async fn volume(&self) -> Result<u8, DlnaError> {
        get_volume(&self.control_url).await
    }

    async fn set_volume(&self, volume: u8) -> Result<(), DlnaError> {
        set_volume(&self.control_url, volume).await
    }

    async fn muted(&self) -> Result<bool, DlnaError> {
        get_mute(&self.control_url).await
    }

    async fn set_muted(&self, muted: bool) -> Result<(), DlnaError> {
        set_mute(&self.control_url, muted).await
    }
}

#[cfg(test)]
mod rendering_control_tests {
    use super::*;

    /// A description with BOTH services, RenderingControl first, so a parser that
    /// takes the first match answers volume for a cast. This is the same trap the
    /// AVTransport test pins from the other side.
    const BOTH_SERVICES: &str = r#"<root><device><serviceList>
        <service><serviceType>urn:schemas-upnp-org:service:RenderingControl:1</serviceType>
        <controlURL>/RenderingControl/ctrl</controlURL></service>
        <service><serviceType>urn:schemas-upnp-org:service:AVTransport:1</serviceType>
        <controlURL>/AVTransport/ctrl</controlURL></service>
    </serviceList></device></root>"#;

    /// A renderer that advertises only AVTransport has no volume control, and that is
    /// a normal renderer rather than a broken one.
    const CAST_ONLY: &str = r#"<root><device><serviceList>
        <service><serviceType>urn:schemas-upnp-org:service:AVTransport:1</serviceType>
        <controlURL>/AVTransport/ctrl</controlURL></service>
    </serviceList></device></root>"#;

    #[test]
    fn the_rendering_control_url_is_the_renderers_own_not_the_first_service() {
        let url = extract_service_control_url(
            BOTH_SERVICES,
            "http://10.0.0.5:8080/desc.xml",
            RENDERING_CONTROL_ACTION,
        )
        .unwrap();
        assert_eq!(url, "http://10.0.0.5:8080/RenderingControl/ctrl");
    }

    #[test]
    fn a_missing_service_is_reported_by_name_so_the_two_absences_are_distinguishable() {
        let error = extract_service_control_url(
            CAST_ONLY,
            "http://10.0.0.5:8080/desc.xml",
            RENDERING_CONTROL_ACTION,
        )
        .expect_err("a cast-only renderer has no RenderingControl");
        match error {
            DlnaError::ServiceNotFound(service) => {
                assert_eq!(
                    service, RENDERING_CONTROL_ACTION,
                    "the error must name WHICH service is absent, or 'no volume' and
                     'cannot cast' become the same failure"
                );
            }
            other => panic!("expected ServiceNotFound, got {other:?}"),
        }
    }

    #[test]
    fn a_volume_response_parses_to_its_number() {
        let xml = r#"<?xml version="1.0"?><s:Envelope><s:Body>
            <u:GetVolumeResponse xmlns:u="urn:schemas-upnp-org:service:RenderingControl:1">
            <CurrentVolume>42</CurrentVolume></u:GetVolumeResponse></s:Body></s:Envelope>"#;
        assert_eq!(parse_volume(xml).unwrap(), 42);
    }

    /// `0` is a silent renderer, which is a real answer and not an error or a
    /// sentinel. A parser that treats it as "no data" would report a muted set as
    /// unreachable.
    #[test]
    fn a_silent_renderer_reports_zero_rather_than_an_error() {
        let xml = r#"<u:GetVolumeResponse><CurrentVolume>0</CurrentVolume></u:GetVolumeResponse>"#;
        assert_eq!(parse_volume(xml).unwrap(), 0);
    }

    /// The UPnP `Volume` variable declares 0..100, so a value outside it is the
    /// renderer lying rather than a volume to store.
    #[test]
    fn a_volume_outside_the_declared_range_is_a_fault() {
        let xml =
            r#"<u:GetVolumeResponse><CurrentVolume>101</CurrentVolume></u:GetVolumeResponse>"#;
        let error = parse_volume(xml).expect_err("101 is outside the declared range");
        assert!(matches!(error, DlnaError::SoapError(_)), "got {error:?}");
    }

    #[test]
    fn a_non_numeric_volume_is_a_fault() {
        let xml =
            r#"<u:GetVolumeResponse><CurrentVolume>loud</CurrentVolume></u:GetVolumeResponse>"#;
        let error = parse_volume(xml).expect_err("a word is not a volume");
        assert!(matches!(error, DlnaError::SoapError(_)), "got {error:?}");
    }

    #[test]
    fn a_missing_current_volume_is_a_fault() {
        let xml = r#"<u:GetVolumeResponse><SomethingElse>42</SomethingElse></u:GetVolumeResponse>"#;
        let error = parse_volume(xml).expect_err("no CurrentVolume is not a volume");
        assert!(matches!(error, DlnaError::SoapError(_)), "got {error:?}");
    }

    /// UPnP encodes a boolean as `0`/`1`; some renderers answer `true`/`false`.
    /// Both spellings are accepted so one is not a fault.
    #[test]
    fn both_boolean_spellings_parse() {
        for (body, expected) in [
            ("<CurrentMute>1</CurrentMute>", true),
            ("<CurrentMute>0</CurrentMute>", false),
            ("<CurrentMute>true</CurrentMute>", true),
            ("<CurrentMute>false</CurrentMute>", false),
        ] {
            let xml = format!("<u:GetMuteResponse>{body}</u:GetMuteResponse>");
            assert_eq!(parse_mute(&xml).unwrap(), expected, "body was {body}");
        }
    }

    #[test]
    fn a_missing_current_mute_is_a_fault() {
        let xml = r#"<u:GetMuteResponse><CurrentVolume>42</CurrentVolume></u:GetMuteResponse>"#;
        let error = parse_mute(xml).expect_err("volume is not mute");
        assert!(matches!(error, DlnaError::SoapError(_)), "got {error:?}");
    }
}
