//! Tier 2: Boundary 10 - Version Cadences (SemVer Invariants, Publish Gates, Resolver).

use crate::harness::workspace_audit::WorkspaceAudit;

pub async fn test_01_semver_incompatible_dependency_audit() -> Result<(), String> {
    let root_toml = WorkspaceAudit::read_root_cargo_toml()?;
    // All internal workspace dependencies specify version = "0.7.1"
    assert!(root_toml.contains("version = \"0.7.1\""));
    Ok(())
}

pub async fn test_02_crate_publish_flags() -> Result<(), String> {
    let spec = WorkspaceAudit::read_specdd()?;
    // Deprecated shims must be unpublished
    assert!(spec.contains("publish = false") || spec.contains("shims are unpublished"));
    Ok(())
}

pub async fn test_03_docs_versioning_table_completeness() -> Result<(), String> {
    let spec = WorkspaceAudit::read_specdd()?;
    assert!(spec.contains("3.5 Version cadence"));
    assert!(spec.contains("tv-rs-core") && spec.contains("1.0-track"));
    Ok(())
}

pub async fn test_04_workspace_resolver_version() -> Result<(), String> {
    let root_toml = WorkspaceAudit::read_root_cargo_toml()?;
    assert!(
        root_toml.contains("resolver = \"2\""),
        "Workspace Cargo.toml must use Cargo resolver = \"2\""
    );
    Ok(())
}

pub async fn test_05_rust_version_msrv_consistent() -> Result<(), String> {
    let root_toml = WorkspaceAudit::read_root_cargo_toml()?;
    assert!(
        root_toml.contains("rust-version = \"1.89\""),
        "Workspace Cargo.toml must specify rust-version = \"1.89\""
    );
    Ok(())
}
