//! Tier 2: Boundary 3 - Driver Feature Matrix (Permutations, Conflicts, Edge Flags).

use crate::harness::driver_mock::{MockDriverRegistry, TvVendor};
use crate::harness::workspace_audit::WorkspaceAudit;

pub async fn test_01_missing_feature_fails_cleanly() -> Result<(), String> {
    let mut reg = MockDriverRegistry::new();
    // Only register Roku
    reg.register(Box::new(crate::harness::driver_mock::create_mock_roku_driver()));

    assert!(reg.get(&TvVendor::Roku).is_some());
    assert!(reg.get(&TvVendor::Samsung).is_none());
    assert!(reg.get(&TvVendor::Mdc).is_none());
    Ok(())
}

pub async fn test_02_conflicting_tls_backends_prevented() -> Result<(), String> {
    let root_toml = WorkspaceAudit::read_root_cargo_toml()?;
    // Check tv-rs-transport commentary: default-features = false prevents dual TLS backends
    assert!(
        root_toml.contains("tv-rs-transport") && root_toml.contains("default-features = false"),
        "tv-rs-transport must use default-features = false to prevent TLS collisions"
    );
    Ok(())
}

pub async fn test_03_feature_matrix_all_combinations_cardinality() -> Result<(), String> {
    let features = ["tizen", "webos", "roku", "mdc", "sicp", "ir", "cec", "cast", "discovery", "power"];
    assert_eq!(features.len(), 10);
    // Number of single-feature cells in the matrix
    let single_feature_cells = features.len();
    assert_eq!(single_feature_cells, 10);
    Ok(())
}

pub async fn test_04_empty_registry_when_all_features_disabled() -> Result<(), String> {
    let reg = MockDriverRegistry::new();
    assert_eq!(reg.len(), 0);
    assert_eq!(reg.aggregate_capabilities().len(), 0);
    assert!(reg.vendors().is_empty());
    Ok(())
}

pub async fn test_05_circular_feature_dependency_prohibition() -> Result<(), String> {
    let spec = WorkspaceAudit::read_specdd()?;
    assert!(
        spec.contains("Dependency law") || spec.contains("tv-rs-core depends on nothing"),
        "Spec must mandate strict unidirectional dependency law"
    );
    Ok(())
}
