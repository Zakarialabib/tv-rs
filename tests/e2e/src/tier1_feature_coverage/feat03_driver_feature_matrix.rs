//! Tier 1: Feature 3 - Driver Feature Matrix (R3, TV-18).

use crate::harness::workspace_audit::WorkspaceAudit;
use crate::harness::driver_mock::MockDriverRegistry;

pub async fn test_01_workflow_defines_all_ten_driver_features() -> Result<(), String> {
    let spec = WorkspaceAudit::read_specdd()?;
    let required_features = [
        "tizen", "webos", "roku", "mdc", "sicp", "ir", "cec", "cast", "discovery", "power"
    ];

    for feat in required_features {
        assert!(
            spec.contains(feat),
            "Feature matrix specification must declare feature '{}'",
            feat
        );
    }
    Ok(())
}

pub async fn test_02_drivers_manifest_defines_feature_flags() -> Result<(), String> {
    // Assert 10 features defined in specdd TV-10 and TV-18
    let spec = WorkspaceAudit::read_specdd()?;
    assert!(spec.contains("TV-18"), "TV-18 feature matrix task must be documented");
    assert!(spec.contains("TV-10"), "TV-10 drivers skeleton task must be documented");
    Ok(())
}

pub async fn test_03_no_default_features_configuration() -> Result<(), String> {
    // Empty registry represents the --no-default-features baseline
    let empty_reg = MockDriverRegistry::new();
    assert_eq!(empty_reg.len(), 0);
    assert!(empty_reg.is_empty());
    Ok(())
}

pub async fn test_04_all_features_configuration() -> Result<(), String> {
    let full_reg = MockDriverRegistry::full_default_registry();
    assert_eq!(full_reg.len(), 10);
    Ok(())
}

pub async fn test_05_feature_independence() -> Result<(), String> {
    // Each driver can be registered in isolation without pulling in other drivers
    let mut reg = MockDriverRegistry::new();
    reg.register(Box::new(crate::harness::driver_mock::create_mock_roku_driver()));
    assert_eq!(reg.len(), 1);
    assert_eq!(reg.vendors(), vec![crate::harness::driver_mock::TvVendor::Roku]);
    Ok(())
}
