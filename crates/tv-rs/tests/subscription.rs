//! Integration tests for facade SubscriptionHandle and GENA event stream.

use std::time::Duration;

use wiremock::matchers::{header, header_exists, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use tv_rs::SubscriptionHandle;
use tv_rs_core::PowerState;
use tv_rs_dlna::TransportState;

const NOTIFY_PLAYING: &str = r#"<?xml version="1.0"?>
<e:propertyset xmlns:e="urn:schemas-upnp-org:event-1-0">
  <e:property>
    <LastChange>&lt;Event xmlns="urn:schemas-upnp-org:metadata-1-0/AVT/"&gt;&lt;InstanceID val="0"&gt;&lt;TransportState val="PLAYING"/&gt;&lt;CurrentTrackURI val="http://10.0.0.2:8088/hls/show.m3u8"/&gt;&lt;/InstanceID&gt;&lt;/Event&gt;</LastChange>
  </e:property>
</e:propertyset>"#;

const NOTIFY_STOPPED: &str = r#"<?xml version="1.0"?>
<e:propertyset xmlns:e="urn:schemas-upnp-org:event-1-0">
  <e:property>
    <LastChange>&lt;Event xmlns="urn:schemas-upnp-org:metadata-1-0/AVT/"&gt;&lt;InstanceID val="0"&gt;&lt;TransportState val="STOPPED"/&gt;&lt;/InstanceID&gt;&lt;/Event&gt;</LastChange>
  </e:property>
</e:propertyset>"#;

#[tokio::test]
async fn subscription_handle_receives_and_broadcasts_events() {
    let server = MockServer::start().await;

    // 1. Mock initial SUBSCRIBE
    Mock::given(method("SUBSCRIBE"))
        .and(path("/evt"))
        .and(header_exists("CALLBACK"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("SID", "uuid:sub-100")
                .insert_header("TIMEOUT", "Second-1800"),
        )
        .expect(1)
        .mount(&server)
        .await;

    // 2. Mock UNSUBSCRIBE
    Mock::given(method("UNSUBSCRIBE"))
        .and(path("/evt"))
        .and(header("SID", "uuid:sub-100"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;

    let evt_url = format!("{}/evt", server.uri());
    let cb_url = "http://10.0.0.2:9999/notify";

    let mut handle = SubscriptionHandle::subscribe("uuid:device-100", &evt_url, cb_url)
        .await
        .expect("subscribe succeeds");

    assert_eq!(handle.udn(), "uuid:device-100");
    assert_eq!(handle.event_sub_url(), evt_url);
    assert_eq!(handle.callback_url(), cb_url);

    let mut receiver = handle.subscribe_events();

    // Process NOTIFY PLAYING
    let event1 = handle.handle_notify(NOTIFY_PLAYING).expect("valid notify");
    assert_eq!(event1.transport.state, TransportState::Playing);
    assert_eq!(event1.state.power.value, Some(PowerState::On));

    let rx_event1 = receiver.recv().await.expect("recv broadcast event");
    assert_eq!(rx_event1, event1);

    // Process NOTIFY STOPPED
    let event2 = handle.handle_notify(NOTIFY_STOPPED).expect("valid notify");
    assert_eq!(event2.transport.state, TransportState::Stopped);
    assert_eq!(event2.state.power.value, Some(PowerState::Standby));

    let rx_event2 = receiver.recv().await.expect("recv broadcast event");
    assert_eq!(rx_event2, event2);

    handle.shutdown().await.expect("shutdown succeeds");
}

#[tokio::test]
async fn subscription_handle_triggers_auto_renewal() {
    let server = MockServer::start().await;

    // Initial SUBSCRIBE returning a 5-second timeout
    Mock::given(method("SUBSCRIBE"))
        .and(path("/evt"))
        .and(header_exists("CALLBACK"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("SID", "uuid:sub-renew-1")
                .insert_header("TIMEOUT", "Second-5"),
        )
        .expect(1)
        .mount(&server)
        .await;

    // Renewal SUBSCRIBE expecting SID header
    Mock::given(method("SUBSCRIBE"))
        .and(path("/evt"))
        .and(header("SID", "uuid:sub-renew-1"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("SID", "uuid:sub-renew-1")
                .insert_header("TIMEOUT", "Second-300"),
        )
        .expect(1..)
        .mount(&server)
        .await;

    // UNSUBSCRIBE
    Mock::given(method("UNSUBSCRIBE"))
        .and(path("/evt"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let evt_url = format!("{}/evt", server.uri());

    let mut handle =
        SubscriptionHandle::subscribe("uuid:device-renew", &evt_url, "http://10.0.0.2:9999")
            .await
            .expect("subscribe succeeds");

    // Timeout is 5s, renew margin fraction is 5 (margin 1s). Renewal deadline is 4s.
    // Sleep 4.2s to trigger renewal.
    tokio::time::sleep(Duration::from_millis(4200)).await;

    let sub = handle.subscription().await.expect("subscription exists");
    assert_eq!(sub.sid, "uuid:sub-renew-1");

    handle.shutdown().await.expect("shutdown succeeds");
}
