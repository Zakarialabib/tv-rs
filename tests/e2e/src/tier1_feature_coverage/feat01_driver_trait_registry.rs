//! Tier 1: Feature 1 - Driver Trait & Registry (R1, R2, TV-10).

use crate::harness::driver_mock::*;

pub async fn test_01_driver_trait_vendor_dispatch() -> Result<(), String> {
    let tizen = create_mock_tizen_driver();
    assert_eq!(tizen.vendor(), TvVendor::Samsung);

    let webos = create_mock_webos_driver();
    assert_eq!(webos.vendor(), TvVendor::Lg);

    let roku = create_mock_roku_driver();
    assert_eq!(roku.vendor(), TvVendor::Roku);

    let mdc = create_mock_mdc_driver();
    assert_eq!(mdc.vendor(), TvVendor::Mdc);
    Ok(())
}

pub async fn test_02_driver_capabilities_reporting() -> Result<(), String> {
    let tizen = create_mock_tizen_driver();
    let caps = tizen.capabilities();
    assert!(caps.contains(&Capability::PowerOn));
    assert!(caps.contains(&Capability::PowerOff));
    assert!(caps.contains(&Capability::KeyDispatch));
    assert!(caps.contains(&Capability::AppLaunch));
    assert!(caps.contains(&Capability::ObservationPush));
    assert!(!caps.contains(&Capability::VideoWallControl));
    Ok(())
}

pub async fn test_03_driver_connect_returns_handle() -> Result<(), String> {
    let mut driver = create_mock_tizen_driver();
    let target = DiscoveredTv {
        id: "samsung-living-room".to_string(),
        vendor: TvVendor::Samsung,
        ip: "192.168.1.100".to_string(),
        port: 8002,
        friendly_name: "Samsung QLED 65".to_string(),
        mac: Some("AA:BB:CC:DD:EE:01".to_string()),
    };

    let handle = driver.connect(&target).await.map_err(|e| format!("{:?}", e))?;
    assert_eq!(handle.device_id(), "samsung-living-room");
    assert!(handle.is_connected());
    Ok(())
}

pub async fn test_04_driver_send_command_ack() -> Result<(), String> {
    let mut driver = create_mock_webos_driver();
    let target = DiscoveredTv {
        id: "lg-bedroom".to_string(),
        vendor: TvVendor::Lg,
        ip: "192.168.1.101".to_string(),
        port: 3000,
        friendly_name: "LG OLED 55".to_string(),
        mac: Some("AA:BB:CC:DD:EE:02".to_string()),
    };

    let _ = driver.connect(&target).await.map_err(|e| format!("{:?}", e))?;
    let ack = driver.send(Command::PowerOn).await.map_err(|e| format!("{:?}", e))?;
    assert!(ack.success);
    assert_eq!(ack.command, Command::PowerOn);

    let ack_key = driver.send(Command::SendKey("ENTER".to_string())).await.map_err(|e| format!("{:?}", e))?;
    assert!(ack_key.success);
    Ok(())
}

pub async fn test_05_registry_enumerates_enabled_drivers() -> Result<(), String> {
    let registry = MockDriverRegistry::full_default_registry();
    assert_eq!(registry.len(), 10);

    let vendors = registry.vendors();
    assert!(vendors.contains(&TvVendor::Samsung));
    assert!(vendors.contains(&TvVendor::Lg));
    assert!(vendors.contains(&TvVendor::Roku));
    assert!(vendors.contains(&TvVendor::Mdc));
    assert!(vendors.contains(&TvVendor::Sicp));
    assert!(vendors.contains(&TvVendor::SonyIr));
    assert!(vendors.contains(&TvVendor::Cec));
    assert!(vendors.contains(&TvVendor::GoogleCast));
    assert!(vendors.contains(&TvVendor::GenericDiscovery));
    assert!(vendors.contains(&TvVendor::GenericPower));
    Ok(())
}
