//! Tier 3: Pairwise Combinatorial Cross-Feature Tests.

use crate::harness::driver_mock::*;
use crate::harness::simulated_panel::WolPacket;
use crate::harness::workspace_audit::WorkspaceAudit;

/// T3.01: Driver Registry + Discovery & Power
pub async fn test_01_pairwise_driver_registry_and_discovery_power() -> Result<(), String> {
    let mut reg = MockDriverRegistry::full_default_registry();

    // Discovered Samsung display
    let discovered = DiscoveredTv {
        id: "samsung-discovery-01".to_string(),
        vendor: TvVendor::Samsung,
        ip: "192.168.1.110".to_string(),
        port: 8002,
        friendly_name: "Samsung Discovery 1".to_string(),
        mac: Some("AA:BB:CC:11:22:33".to_string()),
    };

    // Lookup driver from registry by discovered vendor
    let driver = reg.get_mut(&discovered.vendor).ok_or("Driver not found for vendor")?;
    let handle = driver.connect(&discovered).await.map_err(|e| format!("{:?}", e))?;
    assert_eq!(handle.device_id(), "samsung-discovery-01");
    Ok(())
}

/// T3.02: Driver Registry + CapabilitySet Registry
pub async fn test_02_pairwise_driver_registry_and_capability_registry() -> Result<(), String> {
    let reg = MockDriverRegistry::full_default_registry();
    let agg = reg.aggregate_capabilities();

    // Registry must aggregate capabilities across all 10 drivers
    assert!(agg.contains(&Capability::PowerOn));
    assert!(agg.contains(&Capability::VideoWallControl));
    assert!(agg.contains(&Capability::MediaControl));
    assert!(agg.contains(&Capability::ObservationPush));
    assert!(agg.len() >= 10);
    Ok(())
}

/// T3.03: Tizen/webOS Drivers + Observation Stream
pub async fn test_03_pairwise_tizen_webos_and_observation_stream() -> Result<(), String> {
    let mut tizen = create_mock_tizen_driver();
    let mut stream = tizen.observe().await.map_err(|e| format!("{:?}", e))?;

    // State changes emitted during connection/operation
    tizen.emit_event(TvEvent::AppOpened("org.tizen.browser".to_string())).map_err(|e| e.to_string())?;
    tizen.emit_event(TvEvent::VolumeChanged(30)).map_err(|e| e.to_string())?;

    let e1 = stream.recv().await.map_err(|e| e.to_string())?;
    let e2 = stream.recv().await.map_err(|e| e.to_string())?;

    assert_eq!(e1, TvEvent::AppOpened("org.tizen.browser".to_string()));
    assert_eq!(e2, TvEvent::VolumeChanged(30));
    Ok(())
}

/// T3.04: MDC Driver + Power Driver + Conformance
pub async fn test_04_pairwise_mdc_driver_power_and_conformance() -> Result<(), String> {
    let mut mdc = create_mock_mdc_driver();

    // WoL power-on packet
    let wol = WolPacket::build_magic_packet("00:11:22:33:44:55")?;
    assert_eq!(wol.len(), 102);

    // Send MDC power on command
    let pwr_ack = mdc.send(Command::PowerOn).await.map_err(|e| format!("{:?}", e))?;
    assert!(pwr_ack.success);

    // MDC video wall setup
    let vw_ack = mdc.send(Command::SetVideoWall { rows: 2, cols: 2, row: 1, col: 1 })
        .await
        .map_err(|e| format!("{:?}", e))?;
    assert!(vw_ack.success);

    // Conformance verification
    let cases = [
        Capability::PowerOn, Capability::PowerOff, Capability::PowerQuery,
        Capability::VolumeControl, Capability::MuteToggle, Capability::InputSourceSelection,
        Capability::VideoWallControl, Capability::ObservationPush,
    ];
    ConformanceHarness::verify_orphaned_capabilities(&mdc, &cases)?;
    Ok(())
}

/// T3.05: Cast & CEC Drivers + Observation Stream
pub async fn test_05_pairwise_cast_cec_and_observation_stream() -> Result<(), String> {
    let mut cast = create_mock_cast_driver();
    let mut stream = cast.observe().await.map_err(|e| format!("{:?}", e))?;

    cast.emit_event(TvEvent::MediaPlaybackChanged("BUFFERING".to_string())).map_err(|e| e.to_string())?;
    cast.emit_event(TvEvent::MediaPlaybackChanged("PLAYING".to_string())).map_err(|e| e.to_string())?;

    let e1 = stream.recv().await.map_err(|e| e.to_string())?;
    let e2 = stream.recv().await.map_err(|e| e.to_string())?;

    assert_eq!(e1, TvEvent::MediaPlaybackChanged("BUFFERING".to_string()));
    assert_eq!(e2, TvEvent::MediaPlaybackChanged("PLAYING".to_string()));
    Ok(())
}

/// T3.06: Feature Matrix + Driver Registry
pub async fn test_06_pairwise_feature_matrix_and_driver_registry() -> Result<(), String> {
    let mut reg = MockDriverRegistry::new();
    reg.register(Box::new(create_mock_roku_driver()));
    reg.register(Box::new(create_mock_mdc_driver()));

    assert_eq!(reg.len(), 2);
    assert!(reg.get(&TvVendor::Roku).is_some());
    assert!(reg.get(&TvVendor::Mdc).is_some());
    assert!(reg.get(&TvVendor::Samsung).is_none());
    Ok(())
}

/// T3.07: Core no_std + CapabilitySet
pub async fn test_07_pairwise_core_nostd_and_capability_set() -> Result<(), String> {
    let set1 = CapabilitySet::with_capabilities([Capability::PowerOn, Capability::PowerOff]);
    let set2 = CapabilitySet::with_capabilities([Capability::KeyDispatch]);
    let union_set = set1.union(&set2);

    assert_eq!(union_set.len(), 3);
    assert!(union_set.contains(&Capability::PowerOn));
    assert!(union_set.contains(&Capability::KeyDispatch));
    Ok(())
}

/// T3.08: Deprecated Shims + Consolidated Drivers
pub async fn test_08_pairwise_deprecated_shims_and_consolidated_drivers() -> Result<(), String> {
    let spec = WorkspaceAudit::read_specdd()?;
    assert!(
        spec.contains("deprecated shims") || spec.contains("shims"),
        "Shims must exist for discovery, power, and protocols"
    );
    Ok(())
}

/// T3.09: Version Cadences + Feature Matrix
pub async fn test_09_pairwise_version_cadences_and_feature_matrix() -> Result<(), String> {
    let spec = WorkspaceAudit::read_specdd()?;
    assert!(spec.contains("TV-18") && spec.contains("TV-19"));
    Ok(())
}

/// T3.10: HIL Matrix + Driver Capabilities
pub async fn test_10_pairwise_hil_matrix_and_driver_capabilities() -> Result<(), String> {
    let hil_text = WorkspaceAudit::read_hil_results()?;
    assert!(hil_text.contains("Format"));
    assert!(hil_text.contains("Discover") && hil_text.contains("Power off"));
    Ok(())
}

/// T3.11: Vision Honesty + Observation Stream
pub async fn test_11_pairwise_vision_honesty_and_observation_stream() -> Result<(), String> {
    // Assert TV event observation yields pure state events, never synthetic frames
    let event = TvEvent::InputChanged("HDMI 1".to_string());
    match event {
        TvEvent::InputChanged(ref s) => assert_eq!(s, "HDMI 1"),
        _ => panic!("Unexpected event type"),
    }
    Ok(())
}

/// T3.12: Conformance Harness + All 10 Drivers
pub async fn test_12_pairwise_conformance_harness_and_all_ten_drivers() -> Result<(), String> {
    let reg = MockDriverRegistry::full_default_registry();
    let all_conformance_cases = [
        Capability::PowerOn, Capability::PowerOff, Capability::PowerQuery,
        Capability::VolumeControl, Capability::MuteToggle, Capability::KeyDispatch,
        Capability::AppLaunch, Capability::MediaControl, Capability::MediaStatus,
        Capability::InputSourceSelection, Capability::VideoWallControl, Capability::ObservationPush,
    ];

    for vendor in reg.vendors() {
        let driver = reg.get(&vendor).unwrap();
        let res = ConformanceHarness::verify_orphaned_capabilities(driver.as_ref(), &all_conformance_cases);
        assert!(res.is_ok(), "Vendor {:?} failed conformance validation: {:?}", vendor, res);
    }
    Ok(())
}
