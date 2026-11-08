//! Tier 1: Feature 7 - Cast & CEC Drivers (Honesty) (R1, R3, TV-14).

use crate::harness::driver_mock::*;

pub async fn test_01_cast_driver_implements_driver_trait() -> Result<(), String> {
    let cast = create_mock_cast_driver();
    assert_eq!(cast.vendor(), TvVendor::GoogleCast);
    assert!(cast.capabilities().contains(&Capability::MediaControl));
    assert!(cast.capabilities().contains(&Capability::MediaStatus));
    Ok(())
}

pub async fn test_02_cast_media_status_query_dispatch() -> Result<(), String> {
    let mut cast = create_mock_cast_driver();
    let ack = cast.send(Command::MediaStatus).await.map_err(|e| format!("{:?}", e))?;
    assert!(ack.success);
    assert_eq!(ack.command, Command::MediaStatus);
    Ok(())
}

pub async fn test_03_cast_transport_controls() -> Result<(), String> {
    let mut cast = create_mock_cast_driver();
    let play_ack = cast.send(Command::MediaPlay).await.map_err(|e| format!("{:?}", e))?;
    assert!(play_ack.success);

    let pause_ack = cast.send(Command::MediaPause).await.map_err(|e| format!("{:?}", e))?;
    assert!(pause_ack.success);

    let stop_ack = cast.send(Command::MediaStop).await.map_err(|e| format!("{:?}", e))?;
    assert!(stop_ack.success);
    Ok(())
}

pub async fn test_04_cec_client_implements_driver_trait() -> Result<(), String> {
    let cec = create_mock_cec_driver();
    assert_eq!(cec.vendor(), TvVendor::Cec);
    assert!(cec.capabilities().contains(&Capability::PowerOn));
    assert!(cec.capabilities().contains(&Capability::PowerOff));
    assert!(cec.capabilities().contains(&Capability::InputSourceSelection));
    Ok(())
}

pub async fn test_05_cec_power_and_active_source() -> Result<(), String> {
    let mut cec = create_mock_cec_driver();
    let pwr_ack = cec.send(Command::PowerOn).await.map_err(|e| format!("{:?}", e))?;
    assert!(pwr_ack.success);

    let src_ack = cec.send(Command::SetInput("HDMI1".to_string())).await.map_err(|e| format!("{:?}", e))?;
    assert!(src_ack.success);
    Ok(())
}
