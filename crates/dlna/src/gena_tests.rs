//! Tests for UPnP GENA subscription handling and NOTIFY parsing.
//!
//! Ungated with no optional dependencies, so these execute in a default
//! `cargo test -p tv-rs-dlna`.

use std::time::{Duration, SystemTime};

use crate::gena::{
    callback_header, extract_event_sub_url, parse_timeout_header, Subscription, TransportState,
    REQUESTED_TIMEOUT_SECS,
};
use crate::gena_parse::parse_notify_body;

// ── The doubly-encoded LastChange payload ─────────────────────────────────────

/// A real Samsung-shaped NOTIFY: the LastChange text is an escaped XML document.
const NOTIFY_PLAYING: &str = r#"<?xml version="1.0"?>
<e:propertyset xmlns:e="urn:schemas-upnp-org:event-1-0">
  <e:property>
    <LastChange>&lt;Event xmlns="urn:schemas-upnp-org:metadata-1-0/AVT/"&gt;&lt;InstanceID val="0"&gt;&lt;TransportState val="PLAYING"/&gt;&lt;CurrentTrackURI val="http://10.0.0.2:8088/hls/show.m3u8"/&gt;&lt;PlaybackStorageMedium val="NETWORK"/&gt;&lt;/InstanceID&gt;&lt;/Event&gt;</LastChange>
  </e:property>
</e:propertyset>"#;

#[test]
fn parses_state_out_of_the_doubly_encoded_payload() {
    // The whole point: stopping at the outer document yields the literal string
    // "<Event><InstanceID..." and no state at all.
    let ev = parse_notify_body(NOTIFY_PLAYING).expect("must find transport state");
    assert_eq!(ev.state, TransportState::Playing);
    assert_eq!(
        ev.current_uri.as_deref(),
        Some("http://10.0.0.2:8088/hls/show.m3u8")
    );
    assert_eq!(ev.medium.as_deref(), Some("NETWORK"));
    assert!(!ev.is_error());
}

#[test]
fn detects_a_user_pressing_stop_on_the_tv_remote() {
    // The scenario that motivated GENA — previously callers never learned about this.
    let xml = r#"<e:propertyset xmlns:e="urn:schemas-upnp-org:event-1-0"><e:property>
      <LastChange>&lt;Event&gt;&lt;InstanceID val="0"&gt;&lt;TransportState val="STOPPED"/&gt;&lt;/InstanceID&gt;&lt;/Event&gt;</LastChange>
    </e:property></e:propertyset>"#;
    let ev = parse_notify_body(xml).unwrap();
    assert_eq!(ev.state, TransportState::Stopped);
    assert!(!ev.state.is_active(), "stopped is not an active cast");
}

#[test]
fn a_namespaced_lastchange_element_is_found() {
    // Renderers prefix it variously; matching "<LastChange>" literally misses these.
    for tag in ["LastChange", "e:LastChange", "avt:LastChange"] {
        let xml = format!(
            r#"<propertyset><property><{tag}>&lt;Event&gt;&lt;InstanceID val="0"&gt;&lt;TransportState val="PAUSED_PLAYBACK"/&gt;&lt;/InstanceID&gt;&lt;/Event&gt;</{tag}></property></propertyset>"#
        );
        let ev = parse_notify_body(&xml).unwrap_or_else(|| panic!("{tag} not parsed"));
        assert_eq!(ev.state, TransportState::Paused);
    }
}

#[test]
fn a_track_uri_with_a_query_string_survives_double_unescaping() {
    // '&' arrives as "&amp;amp;" — one level for the outer document, one for the
    // attribute. Missing either yields a broken URL.
    let xml = r#"<propertyset><property><LastChange>&lt;Event&gt;&lt;InstanceID val="0"&gt;&lt;TransportState val="PLAYING"/&gt;&lt;CurrentTrackURI val="http://h/s?a=1&amp;amp;b=2"/&gt;&lt;/InstanceID&gt;&lt;/Event&gt;</LastChange></property></propertyset>"#;
    let ev = parse_notify_body(xml).unwrap();
    assert_eq!(ev.current_uri.as_deref(), Some("http://h/s?a=1&b=2"));
}

#[test]
fn an_unknown_vendor_state_does_not_discard_the_rest_of_the_event() {
    // Renderers ship states outside the spec. Failing the whole parse would throw
    // away a perfectly good track URI.
    let xml = r#"<propertyset><property><LastChange>&lt;Event&gt;&lt;InstanceID val="0"&gt;&lt;TransportState val="CUSTOM_VENDOR_STATE"/&gt;&lt;CurrentTrackURI val="http://h/s"/&gt;&lt;/InstanceID&gt;&lt;/Event&gt;</LastChange></property></propertyset>"#;
    let ev = parse_notify_body(xml).unwrap();
    assert_eq!(ev.state, TransportState::Unknown);
    assert_eq!(ev.current_uri.as_deref(), Some("http://h/s"));
}

#[test]
fn a_transport_error_is_distinguished_from_a_plain_stop() {
    // A failed load and a user pressing stop need different UI.
    let xml = r#"<propertyset><property><LastChange>&lt;Event&gt;&lt;InstanceID val="0"&gt;&lt;TransportState val="STOPPED"/&gt;&lt;TransportErrorCode val="716"/&gt;&lt;/InstanceID&gt;&lt;/Event&gt;</LastChange></property></propertyset>"#;
    let ev = parse_notify_body(xml).unwrap();
    assert!(ev.is_error(), "716 is a failure, not a normal stop");
    assert_eq!(ev.error_code, Some(716));
}

#[test]
fn error_code_zero_is_not_an_error() {
    // Renderers send TransportErrorCode="0" on healthy transitions.
    let xml = r#"<propertyset><property><LastChange>&lt;Event&gt;&lt;InstanceID val="0"&gt;&lt;TransportState val="PLAYING"/&gt;&lt;TransportErrorCode val="0"/&gt;&lt;/InstanceID&gt;&lt;/Event&gt;</LastChange></property></propertyset>"#;
    assert!(!parse_notify_body(xml).unwrap().is_error());
}

#[test]
fn a_notify_about_unrelated_properties_is_not_an_error() {
    // Legitimate traffic we simply do not track; must not be reported as a failure.
    let xml = r#"<e:propertyset xmlns:e="urn:schemas-upnp-org:event-1-0">
      <e:property><Volume>31</Volume></e:property>
    </e:propertyset>"#;
    assert!(parse_notify_body(xml).is_none());
}

#[test]
fn an_empty_lastchange_yields_nothing() {
    let xml = "<propertyset><property><LastChange></LastChange></property></propertyset>";
    assert!(parse_notify_body(xml).is_none());
}

#[test]
fn a_truncated_body_does_not_panic() {
    // A renderer dropping the connection mid-NOTIFY must not take the process down.
    for bad in [
        "<propertyset><property><LastChange>&lt;Event",
        "<propertyset><property>",
        "<",
        "",
        "not xml at all",
    ] {
        let _ = parse_notify_body(bad);
    }
}

// ── TransportState semantics ──────────────────────────────────────────────────

#[test]
fn state_parsing_is_case_insensitive() {
    // The spec says uppercase; real devices send mixed case.
    assert_eq!(TransportState::parse("playing"), TransportState::Playing);
    assert_eq!(TransportState::parse("Playing"), TransportState::Playing);
    assert_eq!(
        TransportState::parse("  PLAYING  "),
        TransportState::Playing
    );
}

#[test]
fn both_pause_spellings_are_recognised() {
    // AVTransport defines PAUSED_PLAYBACK; some renderers send plain PAUSED.
    assert_eq!(
        TransportState::parse("PAUSED_PLAYBACK"),
        TransportState::Paused
    );
    assert_eq!(TransportState::parse("PAUSED"), TransportState::Paused);
}

#[test]
fn transitioning_counts_as_active() {
    // It is the buffering state. Treating it as stopped makes every cast flicker to
    // "ended" moments after starting.
    assert!(TransportState::Transitioning.is_active());
    assert!(TransportState::Playing.is_active());
    assert!(!TransportState::Stopped.is_active());
    assert!(!TransportState::Paused.is_active());
    assert!(!TransportState::NoMedia.is_active());
}

// ── Subscription lifetime ─────────────────────────────────────────────────────

fn sub(timeout_secs: u64, age_secs: u64) -> Subscription {
    Subscription {
        sid: "uuid:test-sid".into(),
        callback_url: "http://10.0.0.2:9999/dlna/events".into(),
        event_sub_url: "http://10.0.0.5:8080/AVTransport/evt".into(),
        timeout: Duration::from_secs(timeout_secs),
        established: SystemTime::now() - Duration::from_secs(age_secs),
    }
}

#[test]
fn a_fresh_subscription_does_not_need_renewal() {
    assert!(!sub(1800, 10).needs_renewal(SystemTime::now()));
}

#[test]
fn renewal_is_due_inside_the_margin() {
    // 1800s grant → 360s margin. At 1500s old, 300s remain.
    assert!(sub(1800, 1500).needs_renewal(SystemTime::now()));
}

#[test]
fn a_lapsed_subscription_reports_both_renewal_and_expiry() {
    // Critical for a starved task (suspend/resume): a clock past the deadline must
    // not read as "still valid".
    let s = sub(1800, 3600);
    let now = SystemTime::now();
    assert!(s.needs_renewal(now), "must not conclude it is still valid");
    assert!(s.is_expired(now));
}

#[test]
fn expiry_and_renewal_are_distinct_states() {
    // Renewal-due can still be renewed with its SID; expired must be re-established.
    let s = sub(1800, 1500);
    let now = SystemTime::now();
    assert!(s.needs_renewal(now));
    assert!(!s.is_expired(now), "still inside the window");
}

// ── TIMEOUT header ────────────────────────────────────────────────────────────

#[test]
fn parses_the_standard_second_form() {
    assert_eq!(
        parse_timeout_header(Some("Second-1800")),
        Duration::from_secs(1800)
    );
    assert_eq!(
        parse_timeout_header(Some("second-600")),
        Duration::from_secs(600)
    );
}

#[test]
fn a_shorter_grant_than_requested_is_honoured() {
    // The renderer decides. Assuming our requested value would let the subscription
    // lapse before the renewal loop wakes.
    assert_eq!(
        parse_timeout_header(Some("Second-120")),
        Duration::from_secs(120)
    );
}

#[test]
fn a_missing_or_broken_header_falls_back_rather_than_failing() {
    // Treating an odd header as "no subscription" would drop events from a device
    // that is actually working.
    let fallback = Duration::from_secs(REQUESTED_TIMEOUT_SECS);
    assert_eq!(parse_timeout_header(None), fallback);
    assert_eq!(parse_timeout_header(Some("garbage")), fallback);
    assert_eq!(parse_timeout_header(Some("Second-")), fallback);
    assert_eq!(parse_timeout_header(Some("Second-0")), fallback);
}

#[test]
fn infinite_is_clamped_so_the_renewal_loop_still_runs() {
    // An "infinite" subscription the device forgets after a reboot would otherwise
    // never be re-established.
    assert_eq!(
        parse_timeout_header(Some("infinite")),
        Duration::from_secs(REQUESTED_TIMEOUT_SECS)
    );
}

// ── Headers and URL resolution ────────────────────────────────────────────────

#[test]
fn the_callback_header_is_bracketed() {
    // GENA requires angle brackets; strict renderers answer 412 without them.
    assert_eq!(
        callback_header("http://10.0.0.2:9999/dlna/events"),
        "<http://10.0.0.2:9999/dlna/events>"
    );
}

const DESC: &str = r#"<root xmlns="urn:schemas-upnp-org:device-1-0"><device>
  <serviceList>
    <service>
      <serviceType>urn:schemas-upnp-org:service:RenderingControl:1</serviceType>
      <controlURL>/RenderingControl/ctrl</controlURL>
      <eventSubURL>/RenderingControl/evt</eventSubURL>
    </service>
    <service>
      <serviceType>urn:schemas-upnp-org:service:AVTransport:1</serviceType>
      <controlURL>/AVTransport/ctrl</controlURL>
      <eventSubURL>/AVTransport/evt</eventSubURL>
    </service>
  </serviceList>
</device></root>"#;

#[test]
fn picks_the_avtransport_event_url_not_another_services() {
    // RenderingControl comes first in the document and has its own eventSubURL, so a
    // parser that takes the first match subscribes to volume changes instead.
    let url = extract_event_sub_url(DESC, "http://10.0.0.5:8080/desc.xml").unwrap();
    assert_eq!(url, "http://10.0.0.5:8080/AVTransport/evt");
}

#[test]
fn an_absolute_event_path_is_joined_to_the_authority_not_the_directory() {
    // "/AVTransport/evt" under a base of ".../dmr/desc.xml" must not become
    // ".../dmr/AVTransport/evt".
    let url = extract_event_sub_url(DESC, "http://10.0.0.5:8080/dmr/desc.xml").unwrap();
    assert_eq!(url, "http://10.0.0.5:8080/AVTransport/evt");
}

#[test]
fn an_already_absolute_event_url_is_left_alone() {
    let desc = r#"<root><service>
      <serviceType>urn:schemas-upnp-org:service:AVTransport:1</serviceType>
      <eventSubURL>http://other.host:1400/evt</eventSubURL>
    </service></root>"#;
    let url = extract_event_sub_url(desc, "http://10.0.0.5:8080/desc.xml").unwrap();
    assert_eq!(url, "http://other.host:1400/evt");
}

#[test]
fn a_renderer_without_avtransport_is_an_error() {
    let desc = r#"<root><service>
      <serviceType>urn:schemas-upnp-org:service:RenderingControl:1</serviceType>
      <eventSubURL>/rc/evt</eventSubURL>
    </service></root>"#;
    assert!(extract_event_sub_url(desc, "http://h/desc.xml").is_err());
}
