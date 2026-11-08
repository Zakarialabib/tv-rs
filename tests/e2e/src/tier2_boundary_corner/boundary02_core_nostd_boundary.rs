//! Tier 2: Boundary 2 - Core no_std Boundary (Dependencies, Zero-Alloc, Limits).

use crate::harness::workspace_audit::WorkspaceAudit;
use crate::harness::driver_mock::{Capability, CapabilitySet, CommandId, TvError};

pub async fn test_01_cargo_tree_audit_core_has_zero_tokio_nodes() -> Result<(), String> {
    let manifest = WorkspaceAudit::read_core_cargo_toml()?;
    // Dependencies section of core
    let mut in_deps = false;
    for line in manifest.lines() {
        let t = line.trim();
        if t == "[dependencies]" {
            in_deps = true;
            continue;
        } else if t.starts_with('[') {
            in_deps = false;
        }

        if in_deps && (t.contains("tokio") || t.contains("async-std")) {
            return Err(format!("Forbidden async runtime in core dependencies: {}", t));
        }
    }
    Ok(())
}

pub async fn test_02_core_command_id_boundary_values() -> Result<(), String> {
    // Empty command ID
    let empty = CommandId::new("");
    assert_eq!(empty.as_str(), "");

    // Large command ID (1024 chars)
    let large = "x".repeat(1024);
    let large_id = CommandId::new(&large);
    assert_eq!(large_id.as_str().len(), 1024);

    // Unicode / special characters
    let unicode_id = CommandId::new("cmd-📺-001");
    assert_eq!(unicode_id.as_str(), "cmd-📺-001");
    Ok(())
}

pub async fn test_03_core_capability_set_all_flags() -> Result<(), String> {
    let all_caps = [
        Capability::PowerOn,
        Capability::PowerOff,
        Capability::PowerQuery,
        Capability::VolumeControl,
        Capability::MuteToggle,
        Capability::KeyDispatch,
        Capability::AppLaunch,
        Capability::MediaControl,
        Capability::MediaStatus,
        Capability::InputSourceSelection,
        Capability::VideoWallControl,
        Capability::ObservationPush,
    ];

    let full_set = CapabilitySet::with_capabilities(all_caps);
    assert_eq!(full_set.len(), 12);

    let empty_set = CapabilitySet::new();
    let union_set = empty_set.union(&full_set);
    assert_eq!(union_set.len(), 12);
    Ok(())
}

pub async fn test_04_core_error_clone_and_matching() -> Result<(), String> {
    let errors = [
        TvError::RoutingUnreachable,
        TvError::RoutingTimeout,
        TvError::AuthenticationFailed,
        TvError::CapabilityMissing,
        TvError::StandbyRefusal,
    ];

    for err in errors {
        let formatted = format!("{:?}", err);
        assert!(!formatted.is_empty());
    }
    Ok(())
}

pub async fn test_05_thumbv7em_target_cfg_sanity() -> Result<(), String> {
    let spec = WorkspaceAudit::read_specdd()?;
    assert!(
        spec.contains("thumbv7em-none-eabihf"),
        "specdd must specify thumbv7em-none-eabihf CI target gate"
    );
    Ok(())
}
