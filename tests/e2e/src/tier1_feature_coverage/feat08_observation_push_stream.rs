//! Tier 1: Feature 8 - Observation Push Stream Architecture (R2, TV-15).

use crate::harness::driver_mock::*;

pub async fn test_01_observation_stream_creation() -> Result<(), String> {
    let mut driver = create_mock_tizen_driver();
    let stream = driver.observe().await.map_err(|e| format!("{:?}", e))?;
    drop(stream);
    Ok(())
}

pub async fn test_02_event_dispatch_to_subscribers() -> Result<(), String> {
    let mut driver = create_mock_tizen_driver();
    let mut stream = driver.observe().await.map_err(|e| format!("{:?}", e))?;

    driver.emit_event(TvEvent::PowerStateChanged(true)).map_err(|e| e.to_string())?;

    let event = stream.recv().await.map_err(|e| e.to_string())?;
    assert_eq!(event, TvEvent::PowerStateChanged(true));
    Ok(())
}

pub async fn test_03_multi_subscriber_fanout() -> Result<(), String> {
    let mut driver = create_mock_webos_driver();
    let mut sub1 = driver.observe().await.map_err(|e| format!("{:?}", e))?;
    let mut sub2 = driver.observe().await.map_err(|e| format!("{:?}", e))?;

    driver.emit_event(TvEvent::VolumeChanged(25)).map_err(|e| e.to_string())?;

    let evt1 = sub1.recv().await.map_err(|e| e.to_string())?;
    let evt2 = sub2.recv().await.map_err(|e| e.to_string())?;

    assert_eq!(evt1, TvEvent::VolumeChanged(25));
    assert_eq!(evt2, TvEvent::VolumeChanged(25));
    Ok(())
}

pub async fn test_04_stream_ordering_guarantee() -> Result<(), String> {
    let mut driver = create_mock_mdc_driver();
    let mut stream = driver.observe().await.map_err(|e| format!("{:?}", e))?;

    driver.emit_event(TvEvent::PowerStateChanged(true)).map_err(|e| e.to_string())?;
    driver.emit_event(TvEvent::VolumeChanged(10)).map_err(|e| e.to_string())?;
    driver.emit_event(TvEvent::VolumeChanged(15)).map_err(|e| e.to_string())?;

    assert_eq!(stream.recv().await.map_err(|e| e.to_string())?, TvEvent::PowerStateChanged(true));
    assert_eq!(stream.recv().await.map_err(|e| e.to_string())?, TvEvent::VolumeChanged(10));
    assert_eq!(stream.recv().await.map_err(|e| e.to_string())?, TvEvent::VolumeChanged(15));
    Ok(())
}

pub async fn test_05_tv_client_observe_delegates_to_driver() -> Result<(), String> {
    let mut driver = create_mock_cast_driver();
    let mut stream = driver.observe().await.map_err(|e| format!("{:?}", e))?;

    driver.emit_event(TvEvent::MediaPlaybackChanged("PLAYING".to_string())).map_err(|e| e.to_string())?;
    let evt = stream.recv().await.map_err(|e| e.to_string())?;
    assert_eq!(evt, TvEvent::MediaPlaybackChanged("PLAYING".to_string()));
    Ok(())
}
