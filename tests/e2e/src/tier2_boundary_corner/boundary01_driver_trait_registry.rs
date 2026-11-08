//! Tier 2: Boundary 1 - Driver Trait & Registry (Limits, Errors, Fallbacks).

use crate::harness::driver_mock::*;

pub async fn test_01_driver_send_unsupported_command_returns_typed_error() -> Result<(), String> {
    let mut tizen = create_mock_tizen_driver();
    // Tizen does not have VideoWallControl capability
    let cmd = Command::SetVideoWall { rows: 2, cols: 2, row: 1, col: 1 };
    let res = tizen.send(cmd).await;

    match res {
        Err(TvError::CapabilityMissing) => Ok(()),
        other => Err(format!("Expected CapabilityMissing, got {:?}", other)),
    }
}

pub async fn test_02_driver_connect_invalid_target_ip() -> Result<(), String> {
    let mut driver = create_mock_tizen_driver();
    let invalid_target = DiscoveredTv {
        id: "bad-tv".to_string(),
        vendor: TvVendor::Samsung,
        ip: "0.0.0.0".to_string(),
        port: 8002,
        friendly_name: "Bad IP".to_string(),
        mac: None,
    };

    let res = driver.connect(&invalid_target).await;
    match res {
        Err(TvError::RoutingUnreachable) => Ok(()),
        other => Err(format!("Expected RoutingUnreachable, got {:?}", other)),
    }
}

pub async fn test_03_registry_lookup_unknown_vendor() -> Result<(), String> {
    let mut reg = MockDriverRegistry::new();
    reg.register(Box::new(create_mock_tizen_driver()));

    assert!(reg.get(&TvVendor::Samsung).is_some());
    assert!(reg.get(&TvVendor::GoogleCast).is_none());
    assert!(reg.get(&TvVendor::Cec).is_none());
    Ok(())
}

pub async fn test_04_driver_handle_dropped_safely() -> Result<(), String> {
    let mut driver = create_mock_webos_driver();
    let target = DiscoveredTv {
        id: "lg-1".to_string(),
        vendor: TvVendor::Lg,
        ip: "192.168.1.50".to_string(),
        port: 3000,
        friendly_name: "LG Panel".to_string(),
        mac: None,
    };

    let mut handle = driver.connect(&target).await.map_err(|e| format!("{:?}", e))?;
    assert!(handle.is_connected());

    handle.disconnect().await.map_err(|e| format!("{:?}", e))?;
    assert!(!handle.is_connected());
    drop(handle);
    Ok(())
}

pub async fn test_05_driver_reconnect_resilience() -> Result<(), String> {
    let mut driver = create_mock_roku_driver();
    let target = DiscoveredTv {
        id: "roku-1".to_string(),
        vendor: TvVendor::Roku,
        ip: "192.168.1.51".to_string(),
        port: 8060,
        friendly_name: "Roku Stick".to_string(),
        mac: None,
    };

    let mut h1 = driver.connect(&target).await.map_err(|e| format!("{:?}", e))?;
    h1.disconnect().await.map_err(|e| format!("{:?}", e))?;

    let h2 = driver.connect(&target).await.map_err(|e| format!("{:?}", e))?;
    assert!(h2.is_connected());
    Ok(())
}
