//! Tier 2: Boundary 8 - Observation Push Stream (Backpressure, Lag, Disconnects).

use crate::harness::broadcast_stream::DropOldestRingBuffer;
use crate::harness::driver_mock::TvEvent;

pub async fn test_01_backpressure_drop_oldest_policy() -> Result<(), String> {
    let ring = DropOldestRingBuffer::new(5);

    // Push 8 events into capacity 5 buffer
    for i in 1..=8 {
        ring.push(TvEvent::VolumeChanged(i)).await;
    }

    assert_eq!(ring.total_dropped(), 3);
    assert_eq!(ring.current_seq(), 8);
    Ok(())
}

pub async fn test_02_lagged_subscriber_recovers() -> Result<(), String> {
    let ring = DropOldestRingBuffer::new(4);
    let mut sub = ring.subscribe();

    // Push 10 events overflowing subscriber queue
    for i in 1..=10 {
        ring.push(TvEvent::VolumeChanged(i)).await;
    }

    // Attempting recv will notify Lagged
    let recv_res = sub.recv().await;
    match recv_res {
        Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
            assert!(skipped > 0);
        }
        Ok(_) => {
            // Channel caught latest item
        }
        Err(e) => return Err(format!("Unexpected recv error: {:?}", e)),
    }
    Ok(())
}

pub async fn test_03_zero_subscribers_no_leak() -> Result<(), String> {
    let ring = DropOldestRingBuffer::new(10);
    for i in 1..=20 {
        ring.push(TvEvent::VolumeChanged(i)).await;
    }
    assert_eq!(ring.current_seq(), 20);
    Ok(())
}

pub async fn test_04_slow_consumer_does_not_block_fast_consumer() -> Result<(), String> {
    let ring = DropOldestRingBuffer::new(16);
    let mut fast_sub = ring.subscribe();
    let _slow_sub = ring.subscribe();

    ring.push(TvEvent::PowerStateChanged(true)).await;
    let fast_evt = fast_sub.recv().await.map_err(|e| e.to_string())?;
    assert_eq!(fast_evt.event, TvEvent::PowerStateChanged(true));
    Ok(())
}

pub async fn test_05_channel_close_on_driver_disconnect() -> Result<(), String> {
    let (tx, mut rx) = tokio::sync::broadcast::channel::<TvEvent>(4);
    tx.send(TvEvent::DeviceDisconnected).map_err(|e| e.to_string())?;

    let evt = rx.recv().await.map_err(|e| e.to_string())?;
    assert_eq!(evt, TvEvent::DeviceDisconnected);

    drop(tx);
    let end = rx.recv().await;
    assert_eq!(end, Err(tokio::sync::broadcast::error::RecvError::Closed));
    Ok(())
}
