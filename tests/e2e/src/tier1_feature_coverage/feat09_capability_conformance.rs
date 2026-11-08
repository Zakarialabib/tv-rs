//! Tier 1: Feature 9 - CapabilitySet Registry & Conformance Suite (R2, TV-16).

use crate::harness::driver_mock::*;

pub async fn test_01_capability_set_from_driver() -> Result<(), String> {
    let tizen = create_mock_tizen_driver();
    let caps = tizen.capabilities();
    assert!(caps.contains(&Capability::PowerOn));
    assert!(caps.contains(&Capability::KeyDispatch));
    assert_eq!(caps.len(), 7);
    Ok(())
}

pub async fn test_02_capability_registry_registration() -> Result<(), String> {
    let mut reg = MockDriverRegistry::new();
    reg.register(Box::new(create_mock_tizen_driver()));
    reg.register(Box::new(create_mock_webos_driver()));

    assert_eq!(reg.len(), 2);
    assert!(reg.vendors().contains(&TvVendor::Samsung));
    assert!(reg.vendors().contains(&TvVendor::Lg));
    Ok(())
}

pub async fn test_03_conformance_case_execution() -> Result<(), String> {
    let mdc = create_mock_mdc_driver();
    let test_cases = [
        Capability::PowerOn,
        Capability::PowerOff,
        Capability::PowerQuery,
        Capability::VolumeControl,
        Capability::MuteToggle,
        Capability::InputSourceSelection,
        Capability::VideoWallControl,
        Capability::ObservationPush,
    ];

    let result = ConformanceHarness::verify_orphaned_capabilities(&mdc, &test_cases);
    assert!(result.is_ok());
    Ok(())
}

pub async fn test_04_capability_subset_query() -> Result<(), String> {
    let roku = create_mock_roku_driver();
    let caps = roku.capabilities();

    assert!(caps.contains(&Capability::KeyDispatch));
    assert!(!caps.contains(&Capability::VideoWallControl));
    assert!(!caps.contains(&Capability::MediaControl));
    Ok(())
}

pub async fn test_05_capability_union_and_aggregation() -> Result<(), String> {
    let reg = MockDriverRegistry::full_default_registry();
    let agg = reg.aggregate_capabilities();

    assert!(agg.contains(&Capability::PowerOn));
    assert!(agg.contains(&Capability::KeyDispatch));
    assert!(agg.contains(&Capability::VideoWallControl));
    assert!(agg.contains(&Capability::MediaControl));
    assert!(agg.contains(&Capability::ObservationPush));
    Ok(())
}
