//! DIDL-Lite metadata generation for DLNA AVTransport `SetAVTransportURI`.
//!
//! A minimal but valid DIDL-Lite item carrying the resource URL the renderer
//! should play. Renderers generally ignore most metadata fields and key off
//! the `<res>` URL, so we keep this small and correct.

/// Build a DIDL-Lite XML fragment for a single media resource.
///
/// `title` is the on-screen label; `url` is the playable stream.
/// `protocol_info` advertises the container — `http-get:*` is the safe generic
/// choice that virtually all renderers accept.
pub fn build_didl_lite(title: &str, url: &str) -> String {
    let safe_title = escape_xml(title);
    let safe_url = escape_xml(url);
    format!(
        r#"<DIDL-Lite xmlns:dc="urn:schemas-upnp-org:metadata-1-0/dc/" xmlns:upnp="urn:schemas-upnp-org:metadata-1-0/upnp/" xmlns:r="urn:schemas-upnp-org:rel-2003-10" xmlns="urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/"><item id="0" parentID="-1" restricted="0"><dc:title>{safe_title}</dc:title><upnp:class>object.item.videoItem</upnp:class><res protocolInfo="http-get:*:video/mp4:*">{safe_url}</res></item></DIDL-Lite>"#
    )
}

/// Escape text for embedding in an XML element or attribute.
///
/// Applied to both `CurrentURI` and the DIDL fragment in `SetAVTransportURI`:
/// the DIDL is embedded as element text inside the SOAP body, and a stream URL
/// with a query string arrives with literal `&` that would otherwise break the
/// envelope.
pub(crate) fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_a_didl_fragment_with_the_url() {
        let didl = build_didl_lite("Lobby", "http://10.0.0.2:8088/hls/show.m3u8");
        assert!(didl.contains(
            "<res protocolInfo=\"http-get:*:video/mp4:*\">http://10.0.0.2:8088/hls/show.m3u8</res>"
        ));
        assert!(didl.contains("<dc:title>Lobby</dc:title>"));
        assert!(didl.contains("object.item.videoItem"));
    }

    #[test]
    fn escapes_the_title() {
        // Titles come from user content; an unescaped label breaks the envelope.
        let didl = build_didl_lite("News & Weather <Live>", "http://h/s");
        assert!(didl.contains("News &amp; Weather &lt;Live&gt;"));
    }

    #[test]
    fn escapes_the_url() {
        let didl = build_didl_lite("t", "http://h/s?a=1&b=2");
        assert!(didl.contains("http://h/s?a=1&amp;b=2"));
    }
}
