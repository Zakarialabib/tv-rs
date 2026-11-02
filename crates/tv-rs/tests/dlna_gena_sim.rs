//! DLNA/GENA event-push exit, against the mock renderer.
//!
//! The G18 handle's unit tests cover the registry and the pure NOTIFY parser;
//! the HTTP halves were covered separately (SUBSCRIBE over `wiremock`, the
//! NOTIFY body as a parsed string). What no test closed until now is the
//! **loop**: the handle subscribes over real HTTP, the renderer delivers a
//! real NOTIFY back over a second, opposite-direction connection, and the
//! event surfaces on the handle's stream. That loop is the feature G18 ships.
//!
//! The mock lives in `crates/sim/src/gena_renderer.rs`; both the renderer and
//! the callback sink are real HTTP servers/clients on real sockets.

use std::time::Duration;

use tv_rs_dlna::{Renderer, RendererEvent, SubscriptionHandle, TransportState};
use tv_rs_sim::{MockGenaRenderer, MockNotifySink};

/// A renderer model pointing at the mock's event endpoint.
fn mock_renderer(udn: &str, renderer: &MockGenaRenderer) -> Renderer {
    Renderer {
        udn: udn.to_string(),
        friendly_name: "Mock Renderer".to_string(),
        manufacturer: "tv-rs-sim".to_string(),
        model_name: "MockGenaRenderer".to_string(),
        ip: renderer.local_addr().ip().to_string(),
        location: String::new(),
        av_transport_url: None,
        rendering_control_url: None,
        event_sub_url: Some(renderer.event_sub_url()),
    }
}

/// The doubly-encoded NOTIFY body a renderer sends on PLAYING, verbatim from
/// `docs/02-playbook.md` §7's recorded shape.
fn playing_notify_body() -> String {
    let inner = r#"<Event xmlns="urn:schemas-upnp-org:event-1-0"><InstanceID val="0"><TransportState val="PLAYING"/></InstanceID></Event>"#;
    let escaped = inner
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    format!(
        r#"<e:propertyset xmlns:e="urn:schemas-upnp-org:event-1-0"><e:property><LastChange>{escaped}</LastChange></e:property></e:propertyset>"#
    )
}

#[tokio::test]
async fn subscribe_notify_loop_surfaces_the_event() {
    let renderer = MockGenaRenderer::start("127.0.0.1:0")
        .await
        .expect("renderer binds");
    let sink = MockNotifySink::start("127.0.0.1:0")
        .await
        .expect("sink binds");

    // The handle subscribes; the mock renderer records the callback URL.
    let handle = SubscriptionHandle::start(
        [mock_renderer("uuid:mock-renderer-1", &renderer)],
        sink.callback_url(),
    )
    .await;

    // Give the initial subscribe a moment, then assert it landed.
    for _ in 0..50 {
        if !renderer.callbacks().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(
        renderer.callbacks().len(),
        1,
        "exactly one establishing SUBSCRIBE"
    );
    assert!(
        renderer.callbacks()[0].contains(&sink.local_addr().to_string()),
        "the CALLBACK header points at the subscriber's sink: {:?}",
        renderer.callbacks()[0]
    );

    // The renderer delivers a NOTIFY to the registered callback URL.
    let body = playing_notify_body();
    let delivered = renderer
        .deliver_notify(&format!("uuid:mock-sid-1"), &body)
        .await;
    assert!(delivered, "the NOTIFY reached the callback sink");

    // The sink feeds `ingest_notify`; the event stream surfaces the state.
    let deliveries = sink.wait_for_deliveries(1, Duration::from_secs(2)).await;
    assert_eq!(deliveries.len(), 1, "exactly one NOTIFY delivery");
    let (sid, received_body) = &deliveries[0];
    assert_eq!(sid, "uuid:mock-sid-1");
    assert!(handle.ingest_notify(sid, received_body), "the SID resolves");

    let mut events = handle.take_events().expect("events stream");
    let event = tokio::time::timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("event arrives")
        .expect("stream open");
    match event {
        RendererEvent::Transport { udn, event } => {
            assert_eq!(udn, "uuid:mock-renderer-1");
            assert_eq!(
                event.state,
                TransportState::Playing,
                "the doubly-encoded payload parsed to PLAYING"
            );
        }
        other => panic!("expected a Transport event, got {other:?}"),
    }

    // Shutdown unsubscribes the live subscription.
    handle.shutdown().await.expect("clean shutdown");
    for _ in 0..50 {
        if !renderer.unsubscribes().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(
        renderer.unsubscribes().len(),
        1,
        "shutdown UNSUBSCRIBEs the live subscription"
    );

    renderer.shutdown().await;
    sink.shutdown().await;
}

#[tokio::test]
async fn notify_for_an_unknown_sid_is_rejected() {
    let renderer = MockGenaRenderer::start("127.0.0.1:0")
        .await
        .expect("renderer binds");
    let sink = MockNotifySink::start("127.0.0.1:0")
        .await
        .expect("sink binds");

    let handle = SubscriptionHandle::start(
        [mock_renderer("uuid:mock-renderer-2", &renderer)],
        sink.callback_url(),
    )
    .await;
    let mut events = handle.take_events().expect("events stream");

    // A NOTIFY with a SID no subscription ever registered is dropped — and it
    // must not surface as an event.
    let body = playing_notify_body();
    assert!(
        !handle.ingest_notify("uuid:never-subscribed", &body),
        "an unknown SID is rejected"
    );

    // And the renderer refuses to deliver for an unknown SID at all.
    assert!(
        !renderer
            .deliver_notify("uuid:never-subscribed", &body)
            .await,
        "no callback was ever registered for this SID"
    );

    // No event surfaced from either path.
    let none = tokio::time::timeout(Duration::from_millis(300), events.recv()).await;
    assert!(none.is_err(), "no event may surface for an unknown SID");

    handle.shutdown().await.expect("clean shutdown");
    renderer.shutdown().await;
    sink.shutdown().await;
}
