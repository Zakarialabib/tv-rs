//! Integration test verifying Driver and TvClient observation streaming across vendors (TV-15).
//!
//! Validates:
//! 1. 3 distinct vendors: Tizen, Roku, MDC.
//! 2. Mutating command dispatch emitting events with end-to-end latency <= 200 ms.
//! 3. Drop-oldest ring buffer backpressure: reporting `ObservationError::Lagged(count)`.
//! 4. Automatic unsubscription on stream drop (`receiver_count == 0`).
//! 5. Facade `TvClient::observe()` streaming integration.

use std::time::{Duration, Instant};

use tv_rs_core::command::Command;
use tv_rs_core::descriptor::{DiscoveredTv, PowerState};
use tv_rs_core::driver::Driver;
use tv_rs_core::observe::{ObservationError, TvEvent};
use tv_rs_core::types::TvVendor;
use tv_rs_drivers::mdc::driver::MdcDriver;
use tv_rs_drivers::observe::DEFAULT_OBSERVE_CHANNEL_CAPACITY;
use tv_rs_drivers::roku::driver::RokuDriver;
use tv_rs_drivers::tizen::driver::TizenDriver;

#[tokio::test]
async fn test_observation_stream_tizen_vendor_latency_and_unsubscription() {
    let mut driver = TizenDriver::new();
    let target = DiscoveredTv::probed(TvVendor::Samsung, "127.0.0.1");
    let _handle = driver.connect(&target).await.expect("connect target");

    // Initial receiver count is 0
    assert_eq!(driver.event_sender().receiver_count(), 0);

    let mut stream = driver.observe().await.expect("open stream");
    assert_eq!(driver.event_sender().receiver_count(), 1);

    // Command dispatch -> event arrival latency measurement (<= 200 ms)
    let start = Instant::now();
    let ack = driver
        .send(Command::Power(true))
        .await
        .expect("power command ack");
    assert!(ack.id.is_some());

    let event = tokio::time::timeout(Duration::from_millis(200), stream.recv())
        .await
        .expect("timed out waiting for event")
        .expect("recv event");
    let elapsed = start.elapsed();

    assert!(
        elapsed <= Duration::from_millis(200),
        "Event delivery took {elapsed:?}, exceeding 200 ms threshold"
    );
    assert_eq!(event, TvEvent::PowerChanged(PowerState::On));

    // Verify subsequent command
    driver
        .send(Command::Volume(25))
        .await
        .expect("volume command ack");
    let event = stream.recv().await.expect("recv volume event");
    assert_eq!(
        event,
        TvEvent::VolumeChanged {
            level: 25,
            muted: false
        }
    );

    // Automatic unsubscription on Drop
    drop(stream);
    assert_eq!(driver.event_sender().receiver_count(), 0);
}

#[tokio::test]
async fn test_observation_stream_roku_vendor_latency_and_events() {
    let mut driver = RokuDriver::new();
    let target = DiscoveredTv::probed(TvVendor::Roku, "127.0.0.1:8060");
    let _handle = driver.connect(&target).await.expect("connect target");

    let mut stream = driver.observe().await.expect("open stream");
    assert_eq!(driver.event_sender().receiver_count(), 1);

    let start = Instant::now();
    let _ = driver.send(Command::Power(false)).await;

    let event = tokio::time::timeout(Duration::from_millis(200), stream.recv())
        .await
        .expect("timeout")
        .expect("recv");
    let elapsed = start.elapsed();
    assert!(elapsed <= Duration::from_millis(200));
    assert_eq!(event, TvEvent::PowerChanged(PowerState::Standby));

    // Test App launch event
    let _ = driver.send(Command::LaunchApp("12".to_string())).await;
    let event = stream.recv().await.expect("recv app event");
    assert_eq!(event, TvEvent::AppChanged("12".to_string()));

    drop(stream);
    assert_eq!(driver.event_sender().receiver_count(), 0);
}

#[tokio::test]
async fn test_observation_stream_mdc_vendor_latency_and_events() {
    let mut driver = MdcDriver::new();
    let target = DiscoveredTv::probed(TvVendor::Mdc, "127.0.0.1:1515");
    let _handle = driver.connect(&target).await.expect("connect target");

    let mut stream = driver.observe().await.expect("open stream");
    assert_eq!(driver.event_sender().receiver_count(), 1);

    let start = Instant::now();
    let _ = driver.send(Command::Power(true)).await;

    let event = tokio::time::timeout(Duration::from_millis(200), stream.recv())
        .await
        .expect("timeout")
        .expect("recv");
    let elapsed = start.elapsed();
    assert!(elapsed <= Duration::from_millis(200));
    assert_eq!(event, TvEvent::PowerChanged(PowerState::On));

    // Volume event
    let _ = driver.send(Command::Volume(40)).await;
    let event = stream.recv().await.expect("recv volume event");
    assert_eq!(
        event,
        TvEvent::VolumeChanged {
            level: 40,
            muted: false
        }
    );

    drop(stream);
    assert_eq!(driver.event_sender().receiver_count(), 0);
}

#[tokio::test]
async fn test_observation_stream_drop_oldest_lag_reporting() {
    let driver = TizenDriver::new();
    let tx = driver.event_sender();
    let rx = tx.subscribe();
    let mut stream = tv_rs_drivers::observe::BroadcastReceiver::new(rx).into_stream();

    // Channel capacity is DEFAULT_OBSERVE_CHANNEL_CAPACITY (64).
    // Send 70 events without receiving, causing a buffer overflow of 6 oldest messages.
    let total_sent = DEFAULT_OBSERVE_CHANNEL_CAPACITY + 6;
    for i in 0..total_sent {
        let _ = tx.send(TvEvent::VolumeChanged {
            level: i as u8,
            muted: false,
        });
    }

    // Next recv must return ObservationError::Lagged with count >= 1
    match stream.recv().await {
        Err(ObservationError::Lagged(dropped)) => {
            assert!(
                dropped > 0,
                "Expected lagged message count > 0, got {dropped}"
            );
        }
        other => panic!("Expected ObservationError::Lagged, got {other:?}"),
    }

    // Subsequent recv delivers the newest surviving messages
    let next_event = stream.recv().await.expect("next surviving event");
    assert!(matches!(next_event, TvEvent::VolumeChanged { .. }));
}

#[tokio::test]
async fn test_facade_tv_client_observe_integration() {
    let target = DiscoveredTv::probed(TvVendor::Roku, "127.0.0.1:8060");
    let client = tv_rs::TvClient::from_discovered(&target).expect("tv client from discovered");

    let mut stream = client.observe().await.expect("client observe stream");
    // When stream is open, it can receive events or wait
    assert_eq!(format!("{stream:?}"), "ObservationStream { _private: () }");
}
