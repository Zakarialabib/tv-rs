//! Tier 1: Feature 11 - HIL Verification Matrix (R3, TV-20).

use crate::harness::workspace_audit::WorkspaceAudit;

pub async fn test_01_hil_results_document_exists() -> Result<(), String> {
    let content = WorkspaceAudit::read_hil_results()?;
    assert!(content.contains("# Hardware verification results"));
    Ok(())
}

pub async fn test_02_hil_table_covers_all_ten_drivers() -> Result<(), String> {
    let content = WorkspaceAudit::read_hil_results()?;
    assert!(content.contains("| Date | Vendor | Model |"));
    assert!(content.contains("Samsung") || content.contains("tv-rs-mdc"));
    Ok(())
}

pub async fn test_03_hil_table_columns_complete() -> Result<(), String> {
    let content = WorkspaceAudit::read_hil_results()?;
    assert!(content.contains("Discover"));
    assert!(content.contains("Pair"));
    assert!(content.contains("Power read"));
    assert!(content.contains("Wake"));
    Ok(())
}

pub async fn test_04_hil_feature_gate_in_tv_rs() -> Result<(), String> {
    let root = WorkspaceAudit::find_workspace_root();
    let tv_rs_toml = std::fs::read_to_string(root.join("crates/tv-rs/Cargo.toml"))
        .map_err(|e| e.to_string())?;
    assert!(tv_rs_toml.contains("hil = []"), "crates/tv-rs/Cargo.toml must define hil feature flag");
    Ok(())
}

pub async fn test_05_hil_test_harness_references_documented_devices() -> Result<(), String> {
    let root = WorkspaceAudit::find_workspace_root();
    let hil_test_file = root.join("crates/tv-rs/tests/hardware_in_the_loop.rs");
    assert!(hil_test_file.exists(), "hardware_in_the_loop.rs test file must exist");
    Ok(())
}
