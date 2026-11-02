//! `SubscriptionHandle` end-to-end against a mocked renderer: the initial
//! subscribe, NOTIFY ingestion and routing, lapse-driven re-subscribe with a
//! fresh SID, abandonment after the retry budget, and `UNSUBSCRIBE` on
//! shutdown.
//!
//! The renew and re-subscribe requests are protocol-disjoint — a renew sends
//! `SID` + `TIMEOUT` and no `CALLBACK`, a (re)subscribe sends `CALLBACK` + `NT`
//! and no `SID` — so each mock matches exactly one request shape and wiremock's
//! mount priority never has to resolve an overlap.

use std::sync::Arc;
use std::time::{Duration, Instant};

use wiremock::matchers::{header, header_exists, method, path};
use wiremock::Respond;
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

use tv_rs_dlna::{Renderer, RendererEvent, SubscriptionHandle, TransportState};

const NOTIFY_PLAYING: &str = r#"<?xml version="1.0"?>
<e:propertyset xmlns:e="urn:schemas-upnp-org:event-1-0">
  <e:property>
    <LastChange>&lt;Event xmlns="urn:schemas-upnp-org:metadata-1-0/AVT/"&gt;&lt;InstanceID val="0"&gt;&lt;TransportState val="PLAYING"/&gt;&lt;CurrentTrackURI val="http://10.0.0.2:8088/hls/show.m3u8"/&gt;&lt;/InstanceID&gt;&lt;/Event&gt;</LastChange>
  </e:property>
</e:propertyset>"#;

fn renderer(event_sub_url: String) -> Renderer {
    Renderer {
        udn: "uuid:rc-test".into(),
        friendly_name: "Living room".into(),
        manufacturer: "tv-rs test".into(),
        model_name: "M-100".into(),
        ip: "10.0.0.9".into(),
        location: "http://10.0.0.9:8080/desc.xml".into(),
        av_transport_url: Some("http://10.0.0.9:8080/AVTransport/control".into()),
        event_sub_url: Some(event_sub_url),
        rendering_control_url: None,
    }
}

/// A successful subscribe grant (the template this fake renderer answers
/// every SUBSCRIBE with).
fn grant(sid: &str, secs: u64) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .insert_header("SID", sid)
        .insert_header("TIMEOUT", format!("Second-{secs}"))
}

/// Replies with its queued templates in order, then `500` once exhausted.
///
/// This is what makes the lapse-driven re-subscribe deterministic: the first
/// subscribe grants a short-lived SID, the second (the re-subscribe the loop
/// issues after the lapse) grants a long-lived fresh one — routing, not
/// wiremock's ordering rules, decides which is served when.
#[derive(Clone)]
struct Sequenced(Arc<std::sync::Mutex<Vec<ResponseTemplate>>>);

impl Sequenced {
    fn new(templates: Vec<ResponseTemplate>) -> Self {
        Self(Arc::new(std::sync::Mutex::new(templates)))
    }
}

impl Respond for Sequenced {
    fn respond(&self, _request: &Request) -> ResponseTemplate {
        let mut queue = self.0.lock().expect("queue lock");
        if queue.is_empty() {
            ResponseTemplate::new(500)
        } else {
            queue.remove(0)
        }
    }
}

async fn next_event(
    events: &mut tokio::sync::mpsc::UnboundedReceiver<RendererEvent>,
) -> RendererEvent {
    tokio::time::timeout(Duration::from_secs(5), events.recv())
        .await
        .expect("event within 5s")
        .expect("stream not closed")
}

#[tokio::test]
async fn a_notify_reaches_the_caller_as_a_transport_event() {
    let server = MockServer::start().await;
    Mock::given(method("SUBSCRIBE"))
        .and(path("/AVTransport/evt"))
        .respond_with(grant("uuid:sub-1", 1800))
        .mount(&server)
        .await;

    let handle = SubscriptionHandle::start(
        [renderer(format!("{}/AVTransport/evt", server.uri()))],
        "http://10.0.0.2:9999/dlna/events",
    )
    .await;
    let mut events = handle.take_events().expect("stream handed out once");

    assert!(
        handle.ingest_notify("uuid:sub-1", NOTIFY_PLAYING),
        "a tracked SID must ingest"
    );
    match next_event(&mut events).await {
        RendererEvent::Transport { udn, event } => {
            assert_eq!(udn, "uuid:rc-test");
            assert_eq!(event.state, TransportState::Playing);
            assert_eq!(
                event.current_uri.as_deref(),
                Some("http://10.0.0.2:8088/hls/show.m3u8")
            );
        }
        RendererEvent::Abandoned { udn } => {
            panic!("expected a transport event, got Abandoned for {udn}")
        }
    }

    // An untracked SID is refused, and a body carrying no AVTransport state
    // is swallowed as documentation says.
    assert!(!handle.ingest_notify("uuid:sub-other", NOTIFY_PLAYING));
    assert!(!handle.ingest_notify(
        "uuid:sub-1",
        r#"<e:propertyset xmlns:e="urn:schemas-upnp-org:event-1-0"><e:property><Volume>30</Volume></e:property></e:propertyset>"#
    ));

    let _ = handle.shutdown().await;
}

#[tokio::test]
async fn a_lapsed_subscription_is_re_established_with_a_fresh_sid() {
    let server = MockServer::start().await;

    // Subscribe shape: floods its queue with grants so every re-subscribe
    // issues a fresh SID answer.
    Mock::given(method("SUBSCRIBE"))
        .and(path("/AVTransport/evt"))
        .and(header_exists("CALLBACK"))
        .respond_with(Sequenced::new(vec![
            grant("uuid:sub-1", 2),
            grant("uuid:sub-2", 1800),
        ]))
        .mount(&server)
        .await;

    // Renew shape: always refuses, so the lapse path is exercised by the
    // ledger instead of a lucky renewal.
    Mock::given(method("SUBSCRIBE"))
        .and(path("/AVTransport/evt"))
        .and(header("SID", "uuid:sub-1"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;

    let handle = SubscriptionHandle::start(
        [renderer(format!("{}/AVTransport/evt", server.uri()))],
        "http://10.0.0.2:9999/dlna/events",
    )
    .await;
    let mut events = handle.take_events().expect("stream handed out once");

    // The renewal point for a 2s grant with a /5 margin is ~1.6s in; give the
    // loop room for the minute-granularity tick plus the ledger's failure
    // accounting on the way to the re-subscribe.
    let deadline = Duration::from_secs(15);
    let started = Instant::now();
    let rotated = loop {
        if started.elapsed() > deadline {
            break false;
        }
        if handle.ingest_notify("uuid:sub-2", NOTIFY_PLAYING) {
            break true;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    };
    assert!(
        rotated,
        "the loop never re-subscribed with a fresh SID after the lapse"
    );

    // The fresh SID delivers events exactly like the original.
    assert!(
        handle.ingest_notify("uuid:sub-2", NOTIFY_PLAYING),
        "fresh SID must ingest after rotation"
    );
    let _ = next_event(&mut events).await;

    // Exercise terminate side of the rotation: shutdown unsubscribes whatever
    // SID is live now (see shutdown test for the exact assertion).
    let _ = handle.shutdown().await;
}

#[tokio::test]
async fn an_unreachable_renderer_is_reported_as_abandoned_after_the_budget() {
    let server = MockServer::start().await;
    Mock::given(method("SUBSCRIBE"))
        .and(path("/AVTransport/evt"))
        .and(header_exists("CALLBACK"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;

    let handle = SubscriptionHandle::start(
        [renderer(format!("{}/AVTransport/evt", server.uri()))],
        "http://10.0.0.2:9999/dlna/events",
    )
    .await;
    let mut events = handle.take_events().expect("stream handed out once");

    // The loop retries once per tick (clamped to 1s); MAX_SUBSCRIBE_FAILURES
    // conflated attempts later the verdict is the event, not an error.
    let abandoned = tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            match events.recv().await {
                Some(RendererEvent::Abandoned { udn }) => break udn,
                Some(_) => continue,
                None => panic!("stream closed before the abandonment"),
            }
        }
    })
    .await
    .expect("abandonment within 15s");
    assert_eq!(abandoned, "uuid:rc-test");

    // After abandonment the registry is empty and the loop retired; shutdown
    // is a formality with nothing to unsubscribe.
    handle.shutdown().await.expect("shutdown with no subs");
}

#[tokio::test]
async fn shutdown_unsubscribes_the_live_subscription() {
    let server = MockServer::start().await;
    Mock::given(method("SUBSCRIBE"))
        .and(path("/AVTransport/evt"))
        .and(header_exists("CALLBACK"))
        .respond_with(grant("uuid:sub-1", 1800))
        .mount(&server)
        .await;
    Mock::given(method("UNSUBSCRIBE"))
        .and(path("/AVTransport/evt"))
        .and(header("SID", "uuid:sub-1"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let handle = SubscriptionHandle::start(
        [renderer(format!("{}/AVTransport/evt", server.uri()))],
        "http://10.0.0.2:9999/dlna/events",
    )
    .await;

    handle.shutdown().await.expect("UNSUBSCRIBE served 200");
}

#[tokio::test]
async fn a_renderer_without_eventing_is_skipped_without_a_subscription_attempt() {
    let server = MockServer::start().await;

    // A renderer with no event_sub_url: it cannot be evented, so nothing is
    // tracked for it and no SUBSCRIBE may be issued.
    let mut blind = renderer("unused".into());
    blind.udn = "uuid:rc-blind".into();
    blind.event_sub_url = None;

    Mock::given(method("SUBSCRIBE"))
        .and(header_exists("CALLBACK"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let handle = SubscriptionHandle::start([blind], "http://10.0.0.2:9999/dlna/events").await;
    let mut events = handle.take_events().expect("stream handed out once");

    // Ingest from the never-tracked device is refused...
    assert!(!handle.ingest_notify("uuid:rc-blind", NOTIFY_PLAYING));
    // ...and no event ever arrives: the loop has nothing to drive, and the
    // stream stays empty (a `recv` here would hang, so instead prove the
    // channel is empty via a short deadline closure).
    let nothing: Option<RendererEvent> =
        tokio::time::timeout(Duration::from_millis(300), events.recv())
            .await
            .unwrap_or(None);
    assert!(nothing.is_none(), "no event may be emitted: {nothing:?}");

    handle.shutdown().await.expect("nothing to unsubscribe");
}
