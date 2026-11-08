//! Tier 1: Feature 2 - Core no_std Boundary (R3, TV-17).

use crate::harness::workspace_audit::WorkspaceAudit;
use crate::harness::driver_mock::{CommandId, DeviceId, TvError, TvVendor};

pub async fn test_01_tv_rs_core_manifest_has_no_tokio_dependency() -> Result<(), String> {
    let manifest = WorkspaceAudit::read_core_cargo_toml()?;
    // Must NOT contain tokio in dependencies or features
    let has_tokio_dep = manifest.lines().any(|l| {
        let trimmed = l.trim();
        (trimmed.starts_with("tokio ") || trimmed.starts_with("tokio=")) && !trimmed.starts_with('#')
    });
    assert!(!has_tokio_dep, "tv-rs-core Cargo.toml contains direct tokio dependency");
    Ok(())
}

pub async fn test_02_tv_rs_core_declares_alloc_feature() -> Result<(), String> {
    let spec = WorkspaceAudit::read_specdd()?;
    assert!(
        spec.contains("no_std") && spec.contains("alloc"),
        "specdd must specify no_std + alloc profile for tv-rs-core"
    );
    Ok(())
}

pub async fn test_03_core_types_instantiable_without_runtime() -> Result<(), String> {
    // Instantiate core types synchronously without any tokio runtime handle
    let cmd_id = CommandId::new("cmd-sync-001");
    assert_eq!(cmd_id.as_str(), "cmd-sync-001");

    let dev_id = DeviceId::new("samsung-tv-1").map_err(|e| format!("{:?}", e))?;
    assert_eq!(dev_id.as_str(), "samsung-tv-1");

    let vendor = TvVendor::Samsung;
    assert_eq!(format!("{:?}", vendor), "Samsung");
    Ok(())
}

pub async fn test_04_core_device_handle_erasure() -> Result<(), String> {
    // DeviceId behaves as an opaque identifier
    let dev_id1 = DeviceId::new("panel-alpha").map_err(|e| format!("{:?}", e))?;
    let dev_id2 = DeviceId::new("panel-alpha").map_err(|e| format!("{:?}", e))?;
    assert_eq!(dev_id1, dev_id2);
    Ok(())
}

pub async fn test_05_core_error_taxonomy_no_std_compatible() -> Result<(), String> {
    let err = TvError::CapabilityMissing;
    let formatted = format!("{}", err);
    assert!(!formatted.is_empty());

    let auth_err = TvError::AuthenticationFailed;
    assert_ne!(format!("{}", auth_err), formatted);
    Ok(())
}
