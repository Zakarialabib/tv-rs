//! Wave 3 Adversarial Challenge & Stress Test Suite (TV-15 Event Observation Streams & Backpressure)
//!
//! Empirically challenges and verifies:
//! 1. Drop-oldest Ring Buffer Overrun:
//!    - Floods broadcast channels with >= 200 events (200, 300, 500) while receiver is slow/halted.
//!    - Verifies `ObservationError::Lagged(u64)` reports the exact dropped count (`dropped == total_sent - capacity`).
//!    - Verifies subsequent `recv()` calls recover, drain surviving messages, and receive fresh broadcasts without stalling or panic.
//!    - Verifies sequential back-to-back buffer overruns recover consistently.
//!    - Verifies ring buffer overrun across all 8 protocol drivers (Tizen, WebOS, Roku, MDC, SICP, IR, CEC, Cast).
//! 2. Unsubscription / Resource Leaks:
//!    - Rapidly subscribes and drops 120+ `ObservationStream` instances concurrently during active high-frequency broadcasts.
//!    - Verifies receiver count transitions strictly back to zero with zero task or memory leaks.
//!    - Multi-wave subscription churn: 5 sequential waves of 64 concurrent subscribers verifying `receiver_count() == 0` after every wave.
//! 3. High-Frequency Fan-out Consistency:
//!    - 16 simultaneous subscribers receiving 50 heterogeneous event sequences at high frequency.
//!    - Asserts byte-for-byte and item-for-item identity across all subscriber streams without drift or reordering.
//!    - Subscriber isolation: slow receiver lagging behind does not degrade, stall, or corrupt fast subscriber.
//! 4. Object-Safety & Bare-Metal Invariance:
//!    - Verifies `format!("{stream:?}") == "ObservationStream { _private: () }"` across all variants.
//!    - Verifies `Box<dyn EventReceiver>` object safety and dynamic dispatch without tokio in core.
//!    - Verifies channel close detection and empty stream behavior.

#![cfg(all(
    feature = "tizen",
    feature = "webos",
    feature = "roku",
    feature = "mdc",
    feature = "sicp",
    feature = "ir",
    feature = "cec",
    feature = "cast"
))]

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex as TokioMutex;
use tokio::sync::Notify;

use async_trait::async_trait;
use tv_rs_core::capability::Input;
use tv_rs_core::descriptor::PowerState;
use tv_rs_core::driver::Driver;
use tv_rs_core::observe::{EventReceiver, ObservationError, ObservationStream, TvEvent};
use tv_rs_drivers::cast::CastDriver;
use tv_rs_drivers::cec::CecDriver;
use tv_rs_drivers::ir::IrDriver;
use tv_rs_drivers::mdc::MdcDriver;
use tv_rs_drivers::observe::{BroadcastReceiver, DEFAULT_OBSERVE_CHANNEL_CAPACITY};
use tv_rs_drivers::registry::DriverRegistry;
use tv_rs_drivers::roku::RokuDriver;
use tv_rs_drivers::sicp::SicpDriver;
use tv_rs_drivers::tizen::TizenDriver;
use tv_rs_drivers::webos::WebOsDriver;

// =============================================================================
// SECTION 1: Drop-oldest Ring Buffer Overrun & Lagged Reporting
// =============================================================================

#[tokio::test]
async fn test_stress_overrun_exact_lagged_count_single_burst() {
    let mut driver = TizenDriver::new();
    let mut stream = driver.observe().await.expect("observe succeeds");
    let tx = driver.event_sender();

    assert_eq!(tx.receiver_count(), 1);

    // Flood channel with exactly 200 events without reading
    let total_sent: u64 = 200;
    let capacity = DEFAULT_OBSERVE_CHANNEL_CAPACITY as u64; // 64
    let expected_dropped = total_sent - capacity; // 136

    for i in 0..total_sent {
        let event = TvEvent::VolumeChanged {
            level: (i % 100) as u8,
            muted: i % 2 == 0,
        };
        let _ = tx.send(event);
    }

    // 1. First recv MUST return ObservationError::Lagged with EXACTLY 136 dropped events
    match stream.recv().await {
        Err(ObservationError::Lagged(dropped)) => {
            assert_eq!(
                dropped, expected_dropped,
                "Expected exact dropped count of {expected_dropped}, got {dropped}"
            );
        }
        other => panic!("Expected ObservationError::Lagged({expected_dropped}), got {other:?}"),
    }

    // 2. Subsequent recvs MUST drain the 64 surviving messages (indices 136..200) in order
    for i in (expected_dropped as usize)..(total_sent as usize) {
        let event = stream.recv().await.expect("recv surviving event");
        assert_eq!(
            event,
            TvEvent::VolumeChanged {
                level: (i % 100) as u8,
                muted: i % 2 == 0,
            },
            "Surviving event at index {i} mismatch"
        );
    }

    // 3. Channel is now empty: broadcasting fresh event must be received immediately without stalling
    let fresh_event = TvEvent::PowerChanged(PowerState::On);
    tx.send(fresh_event.clone()).expect("send fresh event");

    let received = tokio::time::timeout(Duration::from_millis(200), stream.recv())
        .await
        .expect("timed out waiting for fresh event")
        .expect("received fresh event");

    assert_eq!(received, fresh_event);
}

#[tokio::test]
async fn test_stress_overrun_massive_flood_and_sequential_overruns() {
    let mut driver = RokuDriver::new();
    let mut stream = driver.observe().await.expect("observe succeeds");
    let tx = driver.event_sender();

    let capacity = DEFAULT_OBSERVE_CHANNEL_CAPACITY as u64; // 64

    // Pass 1: Flood 500 events
    let flood_1: u64 = 500;
    for i in 0..flood_1 {
        let _ = tx.send(TvEvent::AppChanged(format!("app_{i}")));
    }

    let expected_dropped_1 = flood_1 - capacity; // 436
    match stream.recv().await {
        Err(ObservationError::Lagged(dropped)) => {
            assert_eq!(dropped, expected_dropped_1);
        }
        other => panic!("Expected Lagged({expected_dropped_1}), got {other:?}"),
    }

    // Read 10 surviving events (index 436..446)
    for i in 0..10 {
        let ev = stream.recv().await.expect("recv surviving event");
        let expected_idx = expected_dropped_1 + i;
        assert_eq!(ev, TvEvent::AppChanged(format!("app_{expected_idx}")));
    }

    // Pass 2: Second consecutive flood of 200 events without reading the remaining buffer
    let flood_2: u64 = 200;
    for i in 0..flood_2 {
        let _ = tx.send(TvEvent::AppChanged(format!("second_flood_{i}")));
    }

    // Receiver should lag again and return dropped count
    match stream.recv().await {
        Err(ObservationError::Lagged(dropped)) => {
            assert!(
                dropped > 0,
                "Expected non-zero dropped count on second overrun, got {dropped}"
            );
        }
        other => panic!("Expected Lagged on second overrun, got {other:?}"),
    }

    // Drain all surviving events from second flood
    let mut surviving_second_count = 0;
    while let Ok(ev) = tokio::time::timeout(Duration::from_millis(50), stream.recv()).await {
        if let Ok(TvEvent::AppChanged(id)) = ev {
            assert!(id.starts_with("second_flood_"));
            surviving_second_count += 1;
        } else {
            break;
        }
    }
    assert_eq!(surviving_second_count, DEFAULT_OBSERVE_CHANNEL_CAPACITY);

    // Verify system continues to operate cleanly
    tx.send(TvEvent::PowerChanged(PowerState::Standby))
        .expect("send post-flood event");
    let ev = stream.recv().await.expect("recv post-flood event");
    assert_eq!(ev, TvEvent::PowerChanged(PowerState::Standby));
}

async fn verify_driver_overrun_behavior<D: Driver>(
    mut driver: D,
    tx: &tokio::sync::broadcast::Sender<TvEvent>,
    driver_name: &str,
) {
    let mut stream = driver
        .observe()
        .await
        .unwrap_or_else(|_| panic!("Failed observe() for {driver_name}"));

    let total = 200u64;
    let capacity = DEFAULT_OBSERVE_CHANNEL_CAPACITY as u64;
    let expected_lag = total - capacity;

    for i in 0..total {
        let _ = tx.send(TvEvent::MediaPlaybackChanged(format!("{driver_name}_{i}")));
    }

    match stream.recv().await {
        Err(ObservationError::Lagged(dropped)) => {
            assert_eq!(
                dropped, expected_lag,
                "{driver_name}: expected lag {expected_lag}, got {dropped}"
            );
        }
        other => panic!("{driver_name}: expected Lagged({expected_lag}), got {other:?}"),
    }

    // Drain 1 event
    let ev = stream.recv().await.expect("drain 1 surviving event");
    assert_eq!(
        ev,
        TvEvent::MediaPlaybackChanged(format!("{driver_name}_{expected_lag}"))
    );
}

#[tokio::test]
async fn test_stress_overrun_across_all_eight_protocol_drivers() {
    // 1. Tizen
    let tizen = TizenDriver::new();
    let tx = tizen.event_sender().clone();
    verify_driver_overrun_behavior(tizen, &tx, "Tizen").await;

    // 2. WebOS
    let webos = WebOsDriver::new();
    let tx = webos.event_sender().clone();
    verify_driver_overrun_behavior(webos, &tx, "WebOS").await;

    // 3. Roku
    let roku = RokuDriver::new();
    let tx = roku.event_sender().clone();
    verify_driver_overrun_behavior(roku, &tx, "Roku").await;

    // 4. MDC
    let mdc = MdcDriver::new();
    let tx = mdc.event_sender().clone();
    verify_driver_overrun_behavior(mdc, &tx, "MDC").await;

    // 5. SICP
    let sicp = SicpDriver::new();
    let tx = sicp.event_sender().clone();
    verify_driver_overrun_behavior(sicp, &tx, "SICP").await;

    // 6. IR
    let ir = IrDriver::new();
    let tx = ir.event_sender().clone();
    verify_driver_overrun_behavior(ir, &tx, "IR").await;

    // 7. CEC
    let cec = CecDriver::new();
    let tx = cec.event_sender().clone();
    verify_driver_overrun_behavior(cec, &tx, "CEC").await;

    // 8. Cast
    let cast = CastDriver::new();
    let tx = cast.event_sender().clone();
    verify_driver_overrun_behavior(cast, &tx, "Cast").await;
}

// =============================================================================
// SECTION 2: Unsubscription & Resource Leak Elimination Under Concurrency
// =============================================================================

#[tokio::test]
async fn test_stress_concurrent_rapid_subscribe_drop_with_active_broadcast() {
    let mut driver = TizenDriver::new();
    let tx = driver.event_sender().clone();

    assert_eq!(tx.receiver_count(), 0);

    let stop_notify = Arc::new(Notify::new());
    let stop_notify_clone = stop_notify.clone();

    // High frequency broadcaster background task
    let b_tx = tx.clone();
    let broadcaster = tokio::spawn(async move {
        let mut count: u64 = 0;
        loop {
            tokio::select! {
                _ = stop_notify_clone.notified() => break,
                _ = tokio::task::yield_now() => {
                    let _ = b_tx.send(TvEvent::VolumeChanged {
                        level: (count % 100) as u8,
                        muted: false,
                    });
                    count += 1;
                }
            }
        }
    });

    let num_tasks = 128;
    let mut handles = Vec::with_capacity(num_tasks);

    for task_id in 0..num_tasks {
        let tx_sub = tx.clone();
        handles.push(tokio::spawn(async move {
            let mut stream = BroadcastReceiver::new(tx_sub.subscribe()).into_stream();
            assert_eq!(format!("{stream:?}"), "ObservationStream { _private: () }");

            // Consume a few events, potentially handling lag
            for _ in 0..(task_id % 5) {
                let _ = stream.recv().await;
            }

            // Explicit drop
            drop(stream);
        }));
    }

    for handle in handles {
        handle
            .await
            .expect("subscriber task completed without panic");
    }

    stop_notify.notify_one();
    broadcaster.await.expect("broadcaster stopped cleanly");

    // CRITICAL INVARIANCE: Every dropped stream decrements the receiver count.
    // Receiver count MUST transition back to exactly 0.
    assert_eq!(
        tx.receiver_count(),
        0,
        "Resource leak detected: receiver_count is {}, expected 0",
        tx.receiver_count()
    );

    // Verify driver observe() still operates cleanly after churn
    let stream_after = driver.observe().await.expect("observe after churn");
    assert_eq!(tx.receiver_count(), 1);
    drop(stream_after);
    assert_eq!(tx.receiver_count(), 0);
}

#[tokio::test]
async fn test_stress_multi_wave_subscription_churn() {
    let driver = Arc::new(TokioMutex::new(WebOsDriver::new()));
    let tx = {
        let d = driver.lock().await;
        d.event_sender().clone()
    };

    assert_eq!(tx.receiver_count(), 0);

    let waves = 5;
    let subscribers_per_wave = 64;

    for wave in 0..waves {
        let mut handles = Vec::with_capacity(subscribers_per_wave);
        for _ in 0..subscribers_per_wave {
            let d_arc = driver.clone();
            handles.push(tokio::spawn(async move {
                let stream = {
                    let mut d = d_arc.lock().await;
                    d.observe().await.expect("observe")
                };
                assert_eq!(format!("{stream:?}"), "ObservationStream { _private: () }");
                // Hold briefly then drop
                tokio::time::sleep(Duration::from_millis(5)).await;
                drop(stream);
            }));
        }

        for handle in handles {
            handle.await.expect("wave task finished");
        }

        assert_eq!(
            tx.receiver_count(),
            0,
            "Wave {wave}: receiver_count did not return to 0"
        );
    }
}

// =============================================================================
// SECTION 3: High-Frequency Fan-out Consistency & Subscriber Isolation
// =============================================================================

#[tokio::test]
async fn test_stress_fanout_consistency_multiple_subscribers() {
    let mut driver = MdcDriver::new();
    let tx = driver.event_sender().clone();

    let subscriber_count = 16;
    let event_count = 50;

    let mut streams = Vec::new();
    for _ in 0..subscriber_count {
        streams.push(driver.observe().await.expect("observe stream"));
    }

    assert_eq!(tx.receiver_count(), subscriber_count);

    // Prepare 50 heterogeneous test events
    let mut expected_events = Vec::new();
    for i in 0..event_count {
        let ev = match i % 5 {
            0 => TvEvent::PowerChanged(if i % 2 == 0 {
                PowerState::On
            } else {
                PowerState::Standby
            }),
            1 => TvEvent::VolumeChanged {
                level: (i * 2) as u8,
                muted: i % 10 == 0,
            },
            2 => TvEvent::AppChanged(format!("org.tizen.app.{i}")),
            3 => TvEvent::InputChanged(Input::Hdmi((i % 4 + 1) as u8)),
            _ => TvEvent::MediaPlaybackChanged(format!("TRACK_{i}")),
        };
        expected_events.push(ev);
    }

    // Spawn 16 reader tasks concurrently
    let mut reader_handles = Vec::new();
    for mut stream in streams {
        let count = event_count;
        reader_handles.push(tokio::spawn(async move {
            let mut received = Vec::with_capacity(count);
            for _ in 0..count {
                let ev = stream.recv().await.expect("recv event in fan-out");
                received.push(ev);
            }
            assert_eq!(format!("{stream:?}"), "ObservationStream { _private: () }");
            (received, stream)
        }));
    }

    // Broadcast all 50 events at high frequency
    for ev in &expected_events {
        tx.send(ev.clone()).expect("broadcast event to subscribers");
    }

    // Assert exact byte-for-byte and sequence match across all 16 subscribers
    let mut retained_streams = Vec::new();
    for handle in reader_handles {
        let (received, stream) = handle.await.expect("reader task finished");
        assert_eq!(
            received, expected_events,
            "Subscriber received divergent or corrupt event sequence"
        );
        retained_streams.push(stream);
    }

    assert_eq!(tx.receiver_count(), subscriber_count);

    // Drop all streams and verify complete unsubscription
    drop(retained_streams);
    assert_eq!(tx.receiver_count(), 0);
}

#[tokio::test]
async fn test_stress_fanout_isolation_slow_vs_fast_subscriber() {
    let mut driver = SicpDriver::new();
    let tx = driver.event_sender().clone();

    let mut fast_stream = driver.observe().await.expect("fast stream");
    let mut slow_stream = driver.observe().await.expect("slow stream");

    assert_eq!(tx.receiver_count(), 2);

    let total_events = 200;

    // Fast subscriber reads every event in real-time as it arrives
    let fast_handle = tokio::spawn(async move {
        let mut count = 0;
        for _ in 0..total_events {
            let ev = fast_stream.recv().await.expect("fast recv without lag");
            if let TvEvent::VolumeChanged { level, .. } = ev {
                assert_eq!(level, (count % 100) as u8);
            }
            count += 1;
        }
        (count, fast_stream)
    });

    // Broadcast 200 events
    for i in 0..total_events {
        let ev = TvEvent::VolumeChanged {
            level: (i % 100) as u8,
            muted: false,
        };
        tx.send(ev).expect("send event");
        tokio::task::yield_now().await;
    }

    // 1. Fast subscriber received ALL 200 events without any dropped messages or stalling
    let (fast_count, _fast) = fast_handle.await.expect("fast reader finished");
    assert_eq!(fast_count, 200);

    // 2. Slow subscriber (which did not read) experiences Lagged(136)
    match slow_stream.recv().await {
        Err(ObservationError::Lagged(dropped)) => {
            assert_eq!(dropped, 136);
        }
        other => panic!("Expected slow stream to lag by 136, got {other:?}"),
    }

    // 3. Slow subscriber drains remaining 64 surviving events cleanly
    for _ in 0..64 {
        let ev = slow_stream.recv().await.expect("recv surviving event");
        assert!(matches!(ev, TvEvent::VolumeChanged { .. }));
    }
}

// =============================================================================
// SECTION 4: Object-Safety & Bare-Metal Invariance
// =============================================================================

#[test]
fn test_observation_stream_debug_format_invariance() {
    // 1. Empty new stream
    let s1 = ObservationStream::new();
    assert_eq!(format!("{s1:?}"), "ObservationStream { _private: () }");

    // 2. Default trait stream
    let s2: ObservationStream = Default::default();
    assert_eq!(format!("{s2:?}"), "ObservationStream { _private: () }");

    // 3. BroadcastReceiver backed stream
    let (_tx, rx) = tokio::sync::broadcast::channel::<TvEvent>(16);
    let s3 = BroadcastReceiver::new(rx).into_stream();
    assert_eq!(format!("{s3:?}"), "ObservationStream { _private: () }");

    // 4. Custom boxed EventReceiver
    struct CustomDummy;
    #[async_trait]
    impl EventReceiver for CustomDummy {
        async fn recv(&mut self) -> Result<TvEvent, ObservationError> {
            Err(ObservationError::Closed)
        }
    }
    let s4 = ObservationStream::from_boxed(Box::new(CustomDummy));
    assert_eq!(format!("{s4:?}"), "ObservationStream { _private: () }");
}

struct BareMetalQueueReceiver {
    events: Vec<TvEvent>,
    cursor: usize,
}

#[async_trait]
impl EventReceiver for BareMetalQueueReceiver {
    async fn recv(&mut self) -> Result<TvEvent, ObservationError> {
        if self.cursor < self.events.len() {
            let ev = self.events[self.cursor].clone();
            self.cursor += 1;
            Ok(ev)
        } else {
            Err(ObservationError::Closed)
        }
    }
}

#[tokio::test]
async fn test_bare_metal_event_receiver_dynamic_dispatch() {
    let events = vec![
        TvEvent::ConnectionChanged(true),
        TvEvent::PowerChanged(PowerState::On),
        TvEvent::VolumeChanged {
            level: 25,
            muted: false,
        },
        TvEvent::Disconnected,
    ];

    let custom_rx = BareMetalQueueReceiver {
        events: events.clone(),
        cursor: 0,
    };

    let mut stream = ObservationStream::from_receiver(custom_rx);
    assert_eq!(format!("{stream:?}"), "ObservationStream { _private: () }");

    for expected in &events {
        let actual = stream.recv().await.expect("recv bare-metal event");
        assert_eq!(&actual, expected);
    }

    assert_eq!(stream.recv().await, Err(ObservationError::Closed));
}

#[tokio::test]
async fn test_heterogeneous_event_receiver_vector_dispatch() {
    let streams: Vec<ObservationStream> = vec![
        ObservationStream::new(),
        ObservationStream::from_receiver(BareMetalQueueReceiver {
            events: vec![TvEvent::PowerChanged(PowerState::On)],
            cursor: 0,
        }),
        BroadcastReceiver::new(tokio::sync::broadcast::channel(8).1).into_stream(),
    ];

    assert_eq!(streams.len(), 3);
    for s in &streams {
        assert_eq!(format!("{s:?}"), "ObservationStream { _private: () }");
    }
}

#[tokio::test]
async fn test_driver_registry_dynamic_observe_dispatch() {
    let mut registry = DriverRegistry::with_defaults();
    assert!(!registry.is_empty());

    for driver in registry.drivers_mut() {
        let vendor = driver.vendor();
        let stream = driver
            .observe()
            .await
            .unwrap_or_else(|e| panic!("Driver {vendor:?} observe failed: {e:?}"));
        assert_eq!(
            format!("{stream:?}"),
            "ObservationStream { _private: () }",
            "Format mismatch for driver {vendor:?}"
        );
    }
}

#[tokio::test]
async fn test_observation_channel_closed_detection() {
    let (tx, rx) = tokio::sync::broadcast::channel::<TvEvent>(16);
    let mut stream = BroadcastReceiver::new(rx).into_stream();

    tx.send(TvEvent::ConnectionChanged(false)).unwrap();
    drop(tx);

    let ev = stream
        .recv()
        .await
        .expect("recv pending event before close");
    assert_eq!(ev, TvEvent::ConnectionChanged(false));

    // Next recv must return Closed
    assert_eq!(stream.recv().await, Err(ObservationError::Closed));
    // Repeated recv calls continue returning Closed without panic
    assert_eq!(stream.recv().await, Err(ObservationError::Closed));
}

#[tokio::test]
async fn test_observation_stream_new_is_immediately_closed() {
    let mut stream = ObservationStream::new();
    assert_eq!(stream.recv().await, Err(ObservationError::Closed));
    assert_eq!(stream.recv().await, Err(ObservationError::Closed));
}
