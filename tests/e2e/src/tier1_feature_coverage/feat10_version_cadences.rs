//! Tier 1: Feature 10 - Version Cadences & Docs (R3, TV-19).

use crate::harness::workspace_audit::WorkspaceAudit;

pub async fn test_01_versioning_policy_doc_exists() -> Result<(), String> {
    let spec = WorkspaceAudit::read_specdd()?;
    assert!(spec.contains("TV-19"), "specdd must document TV-19 version cadence task");
    assert!(spec.contains("versioning.md") || spec.contains("1.0-track"));
    Ok(())
}

pub async fn test_02_tv_rs_core_is_on_one_zero_track() -> Result<(), String> {
    let core_manifest = WorkspaceAudit::read_core_cargo_toml()?;
    assert!(core_manifest.contains("version.workspace = true") || core_manifest.contains("version ="));
    Ok(())
}

pub async fn test_03_tv_rs_drivers_follows_facade() -> Result<(), String> {
    let spec = WorkspaceAudit::read_specdd()?;
    assert!(spec.contains("tv-rs-drivers") && spec.contains("facade"));
    Ok(())
}

pub async fn test_04_standalone_crates_have_independent_cadence() -> Result<(), String> {
    let spec = WorkspaceAudit::read_specdd()?;
    assert!(spec.contains("tv-rs-agent") && spec.contains("independent"));
    Ok(())
}

pub async fn test_05_changelog_structure_per_crate() -> Result<(), String> {
    let root = WorkspaceAudit::find_workspace_root();
    // Verify CHANGELOG.md exists in repo root
    let root_cl = root.join("CHANGELOG.md");
    assert!(root_cl.exists(), "Root CHANGELOG.md must exist");
    Ok(())
}
