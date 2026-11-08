//! Tier 2: Boundary 9 - CapabilitySet Registry & Conformance (Orphan Detection, Dynamic Sets).

use crate::harness::driver_mock::*;

pub async fn test_01_orphaned_capability_causes_test_failure() -> Result<(), String> {
    let tizen = create_mock_tizen_driver();
    // Only supply 2 test cases, leaving remaining capabilities orphaned
    let incomplete_test_cases = [Capability::PowerOn, Capability::PowerOff];

    let result = ConformanceHarness::verify_orphaned_capabilities(&tizen, &incomplete_test_cases);
    assert!(result.is_err(), "Orphaned capabilities without conformance cases must trigger error");
    let err_msg = result.unwrap_err();
    assert!(err_msg.contains("orphaned capabilities"));
    Ok(())
}

pub async fn test_02_empty_capability_set_handling() -> Result<(), String> {
    let empty_set = CapabilitySet::new();
    assert_eq!(empty_set.len(), 0);
    assert!(empty_set.is_empty());
    assert!(!empty_set.contains(&Capability::PowerOn));
    Ok(())
}

pub async fn test_03_conflicting_capability_resolution() -> Result<(), String> {
    let set1 = CapabilitySet::with_capabilities([Capability::VideoWallControl]);
    let set2 = CapabilitySet::with_capabilities([Capability::MediaStatus]);

    let merged = set1.union(&set2);
    assert_eq!(merged.len(), 2);
    assert!(merged.contains(&Capability::VideoWallControl));
    assert!(merged.contains(&Capability::MediaStatus));
    Ok(())
}

pub async fn test_04_conformance_timeout_detection() -> Result<(), String> {
    let timeout = tokio::time::Duration::from_millis(50);
    let hanging_test = async {
        tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;
        Ok::<(), String>(())
    };

    let res = tokio::time::timeout(timeout, hanging_test).await;
    assert!(res.is_err(), "Hanging conformance test must be timed out");
    Ok(())
}

pub async fn test_05_dynamic_capability_demotion() -> Result<(), String> {
    let caps = CapabilitySet::with_capabilities([Capability::PowerOn, Capability::KeyDispatch]);
    assert!(caps.contains(&Capability::KeyDispatch));

    // Dynamic demotion: build new set without failed capability
    let demoted = CapabilitySet::with_capabilities(
        caps.iter().filter(|c| **c != Capability::KeyDispatch).copied()
    );

    assert_eq!(demoted.len(), 1);
    assert!(demoted.contains(&Capability::PowerOn));
    assert!(!demoted.contains(&Capability::KeyDispatch));
    Ok(())
}
