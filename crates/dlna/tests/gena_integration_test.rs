//! Integration tests for UPnP GENA eventing, subscription loops, and SOAP controls.

use std::time::{Duration, SystemTime};
use wiremock::matchers::{header, header_exists, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use tv_rs_dlna::{
    cast, parse_notify_body, pause, renew, stop, subscribe_raw, unsubscribe, SubscriptionAction,
    SubscriptionRegistry, TransportState,
};

const NOTIFY_PLAYING: &str = r#"<?xml version="1.0"?>
<e:propertyset xmlns:e="urn:schemas-upnp-org:event-1-0">
  <e:property>
    <LastChange>&lt;Event xmlns="urn:schemas-upnp-org:metadata-1-0/AVT/"&gt;&lt;InstanceID val="0"&gt;&lt;TransportState val="PLAYING"/&gt;&lt;CurrentTrackURI val="http://10.0.0.2:8088/hls/show.m3u8"/&gt;&lt;PlaybackStorageMedium val="NETWORK"/&gt;&lt;/InstanceID&gt;&lt;/Event&gt;</LastChange>
  </e:property>
</e:propertyset>"#;

const NOTIFY_STOPPED: &str = r#"<?xml version="1.0"?>
<e:propertyset xmlns:e="urn:schemas-upnp-org:event-1-0">
  <e:property>
    <LastChange>&lt;Event xmlns="urn:schemas-upnp-org:metadata-1-0/AVT/"&gt;&lt;InstanceID val="0"&gt;&lt;TransportState val="STOPPED"/&gt;&lt;/InstanceID&gt;&lt;/Event&gt;</LastChange>
  </e:property>
</e:propertyset>"#;

#[tokio::test]
async fn subscribe_and_renew_with_various_timeout_headers() {
    let server = MockServer::start().await;

    // 1. Initial SUBSCRIBE request expecting CALLBACK and NT headers.
    Mock::given(method("SUBSCRIBE"))
        .and(path("/AVTransport/evt"))
        .and(header_exists("CALLBACK"))
        .and(header_exists("NT"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("SID", "uuid:sub-12345")
                .insert_header("TIMEOUT", "Second-1800"),
        )
        .expect(1)
        .mount(&server)
        .await;

    let evt_url = format!("{}/AVTransport/evt", server.uri());
    let callback_url = "http://10.0.0.2:9999/dlna/events";

    let sub = subscribe_raw(&evt_url, callback_url)
        .await
        .expect("SUBSCRIBE should succeed");

    assert_eq!(sub.sid, "uuid:sub-12345");
    assert_eq!(sub.timeout, Duration::from_secs(1800));

    // 2. Renewal SUBSCRIBE request expecting SID header and NO CALLBACK.
    Mock::given(method("SUBSCRIBE"))
        .and(path("/AVTransport/evt"))
        .and(header("SID", "uuid:sub-12345"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("SID", "uuid:sub-12345")
                .insert_header("TIMEOUT", "Second-300"),
        )
        .expect(1)
        .mount(&server)
        .await;

    let renewed = renew(&sub).await.expect("renew should succeed");
    assert_eq!(renewed.sid, "uuid:sub-12345");
    assert_eq!(renewed.timeout, Duration::from_secs(300));

    // 3. UNSUBSCRIBE
    Mock::given(method("UNSUBSCRIBE"))
        .and(path("/AVTransport/evt"))
        .and(header("SID", "uuid:sub-12345"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;

    unsubscribe(&renewed)
        .await
        .expect("UNSUBSCRIBE should succeed");
}

#[tokio::test]
async fn subscribe_handles_invalid_sid_or_http_errors() {
    let server = MockServer::start().await;

    // Server returns 200 OK but omits SID
    Mock::given(method("SUBSCRIBE"))
        .and(path("/no-sid"))
        .respond_with(ResponseTemplate::new(200).insert_header("TIMEOUT", "Second-1800"))
        .mount(&server)
        .await;

    let res = subscribe_raw(&format!("{}/no-sid", server.uri()), "http://10.0.0.2:9999").await;
    assert!(res.is_err(), "missing SID must produce an error");

    // Server returns 412 Precondition Failed
    Mock::given(method("SUBSCRIBE"))
        .and(path("/412-err"))
        .respond_with(ResponseTemplate::new(412))
        .mount(&server)
        .await;

    let res = subscribe_raw(&format!("{}/412-err", server.uri()), "http://10.0.0.2:9999").await;
    assert!(res.is_err(), "HTTP 412 must produce an error");
}

#[tokio::test]
async fn test_gena_notify_parsing_and_registry_state_transitions() {
    let mut registry = SubscriptionRegistry::new();
    let udn = "uuid:device-test-1";

    registry.track(udn);
    let now = SystemTime::now();

    // Initial action should be Resubscribe (since pending)
    let due = registry.due(now);
    assert_eq!(due, vec![(udn.to_string(), SubscriptionAction::Resubscribe)]);

    // Simulate establishing a subscription with 600s timeout, established 500s ago (in renewal window)
    let sub = tv_rs_dlna::Subscription {
        sid: "uuid:test-sid-99".to_string(),
        callback_url: "http://10.0.0.2:9999/evt".to_string(),
        event_sub_url: "http://10.0.0.5:8080/evt".to_string(),
        timeout: Duration::from_secs(600),
        established: now - Duration::from_secs(500),
    };

    registry.record_success(udn, sub.clone());
    assert_eq!(registry.udn_for_sid("uuid:test-sid-99"), Some(udn.to_string()));

    // Because it's 500s old out of 600s (renewal margin is 600 / 5 = 120s remaining), it needs renewal
    let due = registry.due(now);
    assert_eq!(due, vec![(udn.to_string(), SubscriptionAction::Renew)]);

    // Test notify parsing for playing state
    let ev_playing = parse_notify_body(NOTIFY_PLAYING).expect("valid notify body");
    assert_eq!(ev_playing.state, TransportState::Playing);
    assert_eq!(
        ev_playing.current_uri.as_deref(),
        Some("http://10.0.0.2:8088/hls/show.m3u8")
    );

    // Test notify parsing for stopped state
    let ev_stopped = parse_notify_body(NOTIFY_STOPPED).expect("valid notify body");
    assert_eq!(ev_stopped.state, TransportState::Stopped);

    // Simulate renew failures leading to Resubscribe
    registry.record_renew_failure(udn);
    registry.record_renew_failure(udn);
    registry.record_renew_failure(udn);

    let due = registry.due(now);
    assert_eq!(due, vec![(udn.to_string(), SubscriptionAction::Resubscribe)]);

    // Simulate subscribe failures leading to Abandon
    for _ in 0..5 {
        registry.record_subscribe_failure(udn);
    }
    let due = registry.due(now);
    assert_eq!(due, vec![(udn.to_string(), SubscriptionAction::Abandon)]);
}

#[tokio::test]
async fn soap_control_actions_wire_mock() {
    let server = MockServer::start().await;

    // SetAVTransportURI & Play
    Mock::given(method("POST"))
        .and(path("/AVTransport/ctrl"))
        .and(header("SOAPACTION", "\"urn:schemas-upnp-org:service:AVTransport:1#SetAVTransportURI\""))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/AVTransport/ctrl"))
        .and(header("SOAPACTION", "\"urn:schemas-upnp-org:service:AVTransport:1#Play\""))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/AVTransport/ctrl"))
        .and(header("SOAPACTION", "\"urn:schemas-upnp-org:service:AVTransport:1#Pause\""))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/AVTransport/ctrl"))
        .and(header("SOAPACTION", "\"urn:schemas-upnp-org:service:AVTransport:1#Stop\""))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let ctrl_url = format!("{}/AVTransport/ctrl", server.uri());

    cast(&ctrl_url, "http://media.example/stream.m3u8", "Test Video")
        .await
        .expect("cast should succeed");

    pause(&ctrl_url).await.expect("pause should succeed");
    stop(&ctrl_url).await.expect("stop should succeed");
}
