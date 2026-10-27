//! UPnP MediaRenderer receiver SOAP request parsing and response generation.
//!
//! Exposes UPnP SOAP request handling for `AVTransport:1` and `ConnectionManager:1`
//! services when running a cast receiver on host/desktop.

use crate::error::DlnaError;
use crate::soap::AV_TRANSPORT_ACTION;

/// ConnectionManager service type used for the `SOAPACTION` header.
pub const CONNECTION_MANAGER_ACTION: &str = "urn:schemas-upnp-org:service:ConnectionManager:1";

/// Default Sink protocol info string advertised by a media receiver.
pub const DEFAULT_SINK_PROTOCOL_INFO: &str =
    "http-get:*:video/mp4:*,http-get:*:video/webm:*,http-get:*:audio/mp3:*,http-get:*:audio/aac:*,http-get:*:image/jpeg:*,http-get:*:image/png:*";

/// Parsed receiver command from an incoming SOAP action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReceiverAction {
    /// `SetAVTransportURI` action with URI and DIDL-Lite metadata.
    SetAvTransportUri {
        /// Target URI to play.
        uri: String,
        /// DIDL-Lite XML metadata.
        metadata: String,
    },
    /// `Play` action with playback speed.
    Play {
        /// Speed string, usually `"1"`.
        speed: String,
    },
    /// `Pause` action.
    Pause,
    /// `Stop` action.
    Stop,
    /// `GetTransportInfo` query.
    GetTransportInfo,
    /// `GetPositionInfo` query.
    GetPositionInfo,
    /// `GetMediaInfo` query.
    GetMediaInfo,
    /// `GetProtocolInfo` query for ConnectionManager.
    GetProtocolInfo,
    /// `GetCurrentConnectionIDs` query for ConnectionManager.
    GetCurrentConnectionIds,
    /// `GetCurrentConnectionInfo` query for ConnectionManager.
    GetCurrentConnectionInfo {
        /// Connection ID requested.
        connection_id: i32,
    },
}

/// State of the cast receiver used to populate response bodies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiverState {
    /// Currently loaded media URI.
    pub current_uri: String,
    /// Currently loaded media metadata.
    pub current_metadata: String,
    /// Transport state (`"PLAYING"`, `"PAUSED_PLAYBACK"`, `"STOPPED"`, `"NO_MEDIA_PRESENT"`).
    pub transport_state: String,
    /// Duration of active track in seconds.
    pub duration_secs: u64,
    /// Current position in active track in seconds.
    pub position_secs: u64,
    /// Sink protocol info supported by receiver.
    pub sink_protocol_info: String,
}

impl Default for ReceiverState {
    fn default() -> Self {
        Self {
            current_uri: String::new(),
            current_metadata: String::new(),
            transport_state: "STOPPED".to_string(),
            duration_secs: 0,
            position_secs: 0,
            sink_protocol_info: DEFAULT_SINK_PROTOCOL_INFO.to_string(),
        }
    }
}

/// Formats seconds into `H+:MM:SS` format.
fn format_hms(secs: u64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

/// Extracts text of `<tag>value</tag>` from XML body.
fn extract_xml_tag(xml: &str, tag: &str) -> Option<String> {
    let open_tag = format!("<{tag}>");
    let close_tag = format!("</{tag}>");
    if let Some(start_idx) = xml.find(&open_tag) {
        let val_start = start_idx + open_tag.len();
        if let Some(end_idx) = xml[val_start..].find(&close_tag) {
            let val = &xml[val_start..val_start + end_idx];
            return Some(
                val.replace("&lt;", "<")
                    .replace("&gt;", ">")
                    .replace("&quot;", "\"")
                    .replace("&apos;", "'")
                    .replace("&amp;", "&")
                    .trim()
                    .to_string(),
            );
        }
    }

    // Try case-insensitive matching if exact tag was not found
    let xml_lower = xml.to_ascii_lowercase();
    let tag_lower = tag.to_ascii_lowercase();
    let open_tag_lower = format!("<{tag_lower}>");
    let close_tag_lower = format!("</{tag_lower}>");
    if let Some(start_idx) = xml_lower.find(&open_tag_lower) {
        let val_start = start_idx + open_tag_lower.len();
        if let Some(end_idx) = xml_lower[val_start..].find(&close_tag_lower) {
            let val = &xml[val_start..val_start + end_idx];
            return Some(
                val.replace("&lt;", "<")
                    .replace("&gt;", ">")
                    .replace("&quot;", "\"")
                    .replace("&apos;", "'")
                    .replace("&amp;", "&")
                    .trim()
                    .to_string(),
            );
        }
    }

    None
}

/// Parses an inbound SOAP request into a [`ReceiverAction`].
///
/// `soap_action` is the `SOAPACTION` HTTP header value (e.g. `"urn:schemas-upnp-org:service:AVTransport:1#SetAVTransportURI"`).
pub fn parse_receiver_soap_request(
    soap_action: &str,
    xml_body: &str,
) -> Result<ReceiverAction, DlnaError> {
    let action_name = soap_action
        .trim_matches('"')
        .split('#')
        .nth(1)
        .unwrap_or(soap_action.trim_matches('"'));

    match action_name {
        "SetAVTransportURI" => {
            let uri = extract_xml_tag(xml_body, "CurrentURI").unwrap_or_default();
            let metadata = extract_xml_tag(xml_body, "CurrentURIMetaData").unwrap_or_default();
            Ok(ReceiverAction::SetAvTransportUri { uri, metadata })
        }
        "Play" => {
            let speed = extract_xml_tag(xml_body, "Speed").unwrap_or_else(|| "1".to_string());
            Ok(ReceiverAction::Play { speed })
        }
        "Pause" => Ok(ReceiverAction::Pause),
        "Stop" => Ok(ReceiverAction::Stop),
        "GetTransportInfo" => Ok(ReceiverAction::GetTransportInfo),
        "GetPositionInfo" => Ok(ReceiverAction::GetPositionInfo),
        "GetMediaInfo" => Ok(ReceiverAction::GetMediaInfo),
        "GetProtocolInfo" => Ok(ReceiverAction::GetProtocolInfo),
        "GetCurrentConnectionIDs" => Ok(ReceiverAction::GetCurrentConnectionIds),
        "GetCurrentConnectionInfo" => {
            let connection_id = extract_xml_tag(xml_body, "ConnectionID")
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(0);
            Ok(ReceiverAction::GetCurrentConnectionInfo { connection_id })
        }
        other => Err(DlnaError::SoapError(format!(
            "Unsupported receiver SOAP action: {other}"
        ))),
    }
}

/// Builds SOAP Envelope wrapper for a inner XML body string.
fn wrap_soap_envelope(service_urn: &str, action: &str, body: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
  <s:Body>
    <u:{action}Response xmlns:u="{service_urn}">
      {body}
    </u:{action}Response>
  </s:Body>
</s:Envelope>"#
    )
}

/// Response for `SetAVTransportURI`.
pub fn build_set_av_transport_uri_response() -> String {
    wrap_soap_envelope(AV_TRANSPORT_ACTION, "SetAVTransportURI", "")
}

/// Response for `Play`.
pub fn build_play_response() -> String {
    wrap_soap_envelope(AV_TRANSPORT_ACTION, "Play", "")
}

/// Response for `Pause`.
pub fn build_pause_response() -> String {
    wrap_soap_envelope(AV_TRANSPORT_ACTION, "Pause", "")
}

/// Response for `Stop`.
pub fn build_stop_response() -> String {
    wrap_soap_envelope(AV_TRANSPORT_ACTION, "Stop", "")
}

/// Response for `GetTransportInfo`.
pub fn build_get_transport_info_response(
    transport_state: &str,
    status: &str,
    speed: &str,
) -> String {
    let body = format!(
        "<CurrentTransportState>{transport_state}</CurrentTransportState><CurrentTransportStatus>{status}</CurrentTransportStatus><CurrentSpeed>{speed}</CurrentSpeed>"
    );
    wrap_soap_envelope(AV_TRANSPORT_ACTION, "GetTransportInfo", &body)
}

/// Response for `GetPositionInfo`.
pub fn build_get_position_info_response(
    track_uri: &str,
    duration_secs: u64,
    rel_time_secs: u64,
) -> String {
    let dur = format_hms(duration_secs);
    let rel = format_hms(rel_time_secs);
    let body = format!(
        "<Track>1</Track><TrackDuration>{dur}</TrackDuration><TrackMetaData></TrackMetaData><TrackURI>{track_uri}</TrackURI><RelTime>{rel}</RelTime><AbsTime>{rel}</AbsTime><RelCount>2147483647</RelCount><AbsCount>2147483647</AbsCount>"
    );
    wrap_soap_envelope(AV_TRANSPORT_ACTION, "GetPositionInfo", &body)
}

/// Response for `GetMediaInfo`.
pub fn build_get_media_info_response(
    media_uri: &str,
    metadata: &str,
    duration_secs: u64,
) -> String {
    let dur = format_hms(duration_secs);
    let body = format!(
        "<NrTracks>1</NrTracks><MediaDuration>{dur}</MediaDuration><CurrentURI>{media_uri}</CurrentURI><CurrentURIMetaData>{metadata}</CurrentURIMetaData><NextURI></NextURI><NextURIMetaData></NextURI><PlayMedium>NETWORK</PlayMedium><RecordMedium>NOT_IMPLEMENTED</RecordMedium><WriteStatus>NOT_WRITABLE</WriteStatus>"
    );
    wrap_soap_envelope(AV_TRANSPORT_ACTION, "GetMediaInfo", &body)
}

/// Response for `GetProtocolInfo`.
pub fn build_get_protocol_info_response(source: &str, sink: &str) -> String {
    let body = format!("<Source>{source}</Source><Sink>{sink}</Sink>");
    wrap_soap_envelope(CONNECTION_MANAGER_ACTION, "GetProtocolInfo", &body)
}

/// Response for `GetCurrentConnectionIDs`.
pub fn build_get_current_connection_ids_response(connection_ids: &str) -> String {
    let body = format!("<ConnectionIDs>{connection_ids}</ConnectionIDs>");
    wrap_soap_envelope(CONNECTION_MANAGER_ACTION, "GetCurrentConnectionIDs", &body)
}

/// Response for `GetCurrentConnectionInfo`.
#[allow(clippy::too_many_arguments)]
pub fn build_get_current_connection_info_response(
    _connection_id: i32,
    rcs_id: i32,
    av_transport_id: i32,
    protocol_info: &str,
    peer_connection_manager: &str,
    peer_connection_id: i32,
    direction: &str,
    status: &str,
) -> String {
    let body = format!(
        "<RcsID>{rcs_id}</RcsID><AVTransportID>{av_transport_id}</AVTransportID><ProtocolInfo>{protocol_info}</ProtocolInfo><PeerConnectionManager>{peer_connection_manager}</PeerConnectionManager><PeerConnectionID>{peer_connection_id}</PeerConnectionID><Direction>{direction}</Direction><Status>{status}</Status>"
    );
    wrap_soap_envelope(CONNECTION_MANAGER_ACTION, "GetCurrentConnectionInfo", &body)
}

/// Dispatches an incoming receiver SOAP request and produces the response XML.
pub fn handle_receiver_soap_request(
    soap_action: &str,
    xml_body: &str,
    state: &ReceiverState,
) -> Result<(ReceiverAction, String), DlnaError> {
    let action = parse_receiver_soap_request(soap_action, xml_body)?;
    let xml_response = match &action {
        ReceiverAction::SetAvTransportUri { .. } => build_set_av_transport_uri_response(),
        ReceiverAction::Play { .. } => build_play_response(),
        ReceiverAction::Pause => build_pause_response(),
        ReceiverAction::Stop => build_stop_response(),
        ReceiverAction::GetTransportInfo => {
            build_get_transport_info_response(&state.transport_state, "OK", "1")
        }
        ReceiverAction::GetPositionInfo => build_get_position_info_response(
            &state.current_uri,
            state.duration_secs,
            state.position_secs,
        ),
        ReceiverAction::GetMediaInfo => build_get_media_info_response(
            &state.current_uri,
            &state.current_metadata,
            state.duration_secs,
        ),
        ReceiverAction::GetProtocolInfo => {
            build_get_protocol_info_response("", &state.sink_protocol_info)
        }
        ReceiverAction::GetCurrentConnectionIds => build_get_current_connection_ids_response("0"),
        ReceiverAction::GetCurrentConnectionInfo { connection_id } => {
            build_get_current_connection_info_response(
                *connection_id,
                0,
                0,
                &state.sink_protocol_info,
                "/",
                -1,
                "Input",
                "OK",
            )
        }
    };

    Ok((action, xml_response))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_set_av_transport_uri() {
        let action_hdr = "\"urn:schemas-upnp-org:service:AVTransport:1#SetAVTransportURI\"";
        let xml = r#"<u:SetAVTransportURI xmlns:u="urn:schemas-upnp-org:service:AVTransport:1">
            <InstanceID>0</InstanceID>
            <CurrentURI>http://192.168.1.10:8000/media.mp4</CurrentURI>
            <CurrentURIMetaData>&lt;DIDL-Lite&gt;Test&lt;/DIDL-Lite&gt;</CurrentURIMetaData>
        </u:SetAVTransportURI>"#;

        let parsed = parse_receiver_soap_request(action_hdr, xml).unwrap();
        assert_eq!(
            parsed,
            ReceiverAction::SetAvTransportUri {
                uri: "http://192.168.1.10:8000/media.mp4".to_string(),
                metadata: "<DIDL-Lite>Test</DIDL-Lite>".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_play_pause_stop() {
        let play_hdr = "\"urn:schemas-upnp-org:service:AVTransport:1#Play\"";
        let play_xml = "<u:Play><Speed>1</Speed></u:Play>";
        assert_eq!(
            parse_receiver_soap_request(play_hdr, play_xml).unwrap(),
            ReceiverAction::Play {
                speed: "1".to_string()
            }
        );

        let pause_hdr = "\"urn:schemas-upnp-org:service:AVTransport:1#Pause\"";
        assert_eq!(
            parse_receiver_soap_request(pause_hdr, "").unwrap(),
            ReceiverAction::Pause
        );

        let stop_hdr = "\"urn:schemas-upnp-org:service:AVTransport:1#Stop\"";
        assert_eq!(
            parse_receiver_soap_request(stop_hdr, "").unwrap(),
            ReceiverAction::Stop
        );
    }

    #[test]
    fn test_handle_receiver_soap_get_transport_info() {
        let state = ReceiverState {
            transport_state: "PLAYING".to_string(),
            ..Default::default()
        };

        let action_hdr = "\"urn:schemas-upnp-org:service:AVTransport:1#GetTransportInfo\"";
        let (action, resp_xml) = handle_receiver_soap_request(action_hdr, "", &state).unwrap();

        assert_eq!(action, ReceiverAction::GetTransportInfo);
        assert!(resp_xml.contains("<CurrentTransportState>PLAYING</CurrentTransportState>"));
        assert!(resp_xml.contains("<CurrentTransportStatus>OK</CurrentTransportStatus>"));
    }

    #[test]
    fn test_connection_manager_protocol_info() {
        let state = ReceiverState::default();
        let action_hdr = "\"urn:schemas-upnp-org:service:ConnectionManager:1#GetProtocolInfo\"";

        let (action, resp_xml) = handle_receiver_soap_request(action_hdr, "", &state).unwrap();

        assert_eq!(action, ReceiverAction::GetProtocolInfo);
        assert!(resp_xml.contains("<Sink>http-get:*:video/mp4:*"));
    }
}
