//! AVTransport SOAP control for DLNA/UPnP MediaRenderers.
//!
//! Given a control URL (extracted from the device description during discovery),
//! this module sends SOAP `SetAVTransportURI` / `Play` / `Pause` / `Stop` / `Seek`
//! actions.
//!
//! Two flavours are offered:
//!
//! - free functions ([`cast`], [`stop`], [`pause`], [`seek`]) for one-shot use,
//! - [`SoapAvTransport`] implementing the [`AvTransport`] trait, for callers that
//!   hold a renderer handle and want a typed control surface.

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

use std::time::Duration;

use async_trait::async_trait;

use crate::error::DlnaError;
use crate::metadata::escape_xml;

/// AVTransport service type used for the `SOAPACTION` header.
pub const AV_TRANSPORT_ACTION: &str = "urn:schemas-upnp-org:service:AVTransport:1";

/// Product token advertised to renderers.
///
/// `DLNADOC/1.50` matters: some renderers gate behaviour on seeing a
/// DLNA-document version in the user agent.
const USER_AGENT: &str = "tv-rs-dlna/0.1.0 DLNADOC/1.50";

/// Find the AVTransport service's `controlURL` within a device description XML.
///
/// Returns the absolute control URL (resolved against the description base if
/// the `controlURL` is relative, which UPnP permits).
pub fn extract_av_transport_control_url(
    description_xml: &str,
    base_url: &str,
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
                    if service_type.trim() == AV_TRANSPORT_ACTION && !control_url.is_empty() {
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

    Err(DlnaError::AvTransportNotFound)
}

/// Strip a namespace prefix from an element name.
fn local_name(qname: &[u8]) -> String {
    let s = String::from_utf8_lossy(qname);
    s.rsplit(':').next().unwrap_or(&s).to_string()
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
async fn post_soap(control_url: &str, action: &str, body: &str) -> Result<String, DlnaError> {
    let envelope = format!(
        r#"<?xml version="1.0" encoding="utf-8"?><s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/"><s:Body>{body}</s:Body></s:Envelope>"#
    );
    let soap_action = format!("\"{AV_TRANSPORT_ACTION}#{action}\"");

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

async fn send_soap(control_url: &str, action: &str, body: &str) -> Result<(), DlnaError> {
    post_soap(control_url, action, body).await.map(|_| ())
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
    send_soap(control_url, "SetAVTransportURI", &set_body).await
}

/// `Play` the currently-set URI at normal speed.
pub async fn play(control_url: &str) -> Result<(), DlnaError> {
    let play_body = format!(
        "<u:Play xmlns:u=\"{AV_TRANSPORT_ACTION}\"><InstanceID>0</InstanceID><Speed>1</Speed></u:Play>",
    );
    send_soap(control_url, "Play", &play_body).await
}

/// `Stop` the current playback on the renderer.
pub async fn stop(control_url: &str) -> Result<(), DlnaError> {
    let stop_body =
        format!("<u:Stop xmlns:u=\"{AV_TRANSPORT_ACTION}\"><InstanceID>0</InstanceID></u:Stop>",);
    send_soap(control_url, "Stop", &stop_body).await
}

/// `Pause` the current playback.
pub async fn pause(control_url: &str) -> Result<(), DlnaError> {
    let pause_body =
        format!("<u:Pause xmlns:u=\"{AV_TRANSPORT_ACTION}\"><InstanceID>0</InstanceID></u:Pause>",);
    send_soap(control_url, "Pause", &pause_body).await
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
    send_soap(control_url, "Seek", &seek_body).await
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
    let response = post_soap(control_url, "GetTransportInfo", &body).await?;
    parse_transport_info(&response)
}

/// `GetPositionInfo`: how far into the track a renderer is.
pub async fn get_position_info(control_url: &str) -> Result<PositionInfo, DlnaError> {
    let body = format!(
        "<u:GetPositionInfo xmlns:u=\"{AV_TRANSPORT_ACTION}\"><InstanceID>0</InstanceID></u:GetPositionInfo>",
    );
    let response = post_soap(control_url, "GetPositionInfo", &body).await?;
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
        let url = extract_av_transport_control_url(DESC, "http://10.0.0.5:8080/desc.xml").unwrap();
        assert_eq!(url, "http://10.0.0.5:8080/AVTransport/ctrl");
    }

    #[test]
    fn a_relative_control_path_is_joined_to_the_authority_not_the_directory() {
        let url =
            extract_av_transport_control_url(DESC, "http://10.0.0.5:8080/dmr/desc.xml").unwrap();
        assert_eq!(url, "http://10.0.0.5:8080/AVTransport/ctrl");
    }

    #[test]
    fn an_already_absolute_control_url_is_left_alone() {
        let desc = r#"<root><service>
        <serviceType>urn:schemas-upnp-org:service:AVTransport:1</serviceType>
        <controlURL>http://other.host:1400/ctrl</controlURL>
        </service></root>"#;
        let url = extract_av_transport_control_url(desc, "http://10.0.0.5:8080/desc.xml").unwrap();
        assert_eq!(url, "http://other.host:1400/ctrl");
    }

    #[test]
    fn a_renderer_without_avtransport_is_an_error() {
        let desc = r#"<root><service>
        <serviceType>urn:schemas-upnp-org:service:RenderingControl:1</serviceType>
        <controlURL>/rc/ctrl</controlURL>
        </service></root>"#;
        assert!(extract_av_transport_control_url(desc, "http://h/desc.xml").is_err());
    }

    #[test]
    fn namespaced_elements_are_parsed() {
        // Some vendors prefix service children (e:serviceType); local-name matching
        // handles both spellings.
        let desc = r#"<root xmlns:e="urn:schemas-upnp-org:device-1-0"><e:service>
        <e:serviceType>urn:schemas-upnp-org:service:AVTransport:1</e:serviceType>
        <e:controlURL>/AVTransport/ctrl</e:controlURL>
        </e:service></root>"#;
        let url = extract_av_transport_control_url(desc, "http://h/desc.xml").unwrap();
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
