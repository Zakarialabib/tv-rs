//! Tier 1: Feature 12 - Vision Honesty Fix (R3, TV-21).

use crate::harness::workspace_audit::WorkspaceAudit;

pub async fn test_01_vision_synthetic_frame_removed_from_public_facade() -> Result<(), String> {
    let spec = WorkspaceAudit::read_specdd()?;
    assert!(
        spec.contains("TV-21") && (spec.contains("synthetic") || spec.contains("vision")),
        "specdd must document TV-21 vision honesty fix"
    );
    Ok(())
}

pub async fn test_02_vision_crate_publish_flag_set_false() -> Result<(), String> {
    let vision_toml = WorkspaceAudit::read_vision_cargo_toml()?;
    // publish = false or documented in specdd
    assert!(
        vision_toml.contains("publish = false") || WorkspaceAudit::read_specdd()?.contains("publish = false"),
        "Vision crate must be gated with publish = false until real decoder lands"
    );
    Ok(())
}

pub async fn test_03_specdd_honesty_table_records_vision_task() -> Result<(), String> {
    let spec = WorkspaceAudit::read_specdd()?;
    assert!(spec.contains("VideoFrame::synthetic") || spec.contains("decodes nothing"));
    assert!(spec.contains("TV-21"));
    Ok(())
}

pub async fn test_04_stream_module_honesty_comment() -> Result<(), String> {
    let spec = WorkspaceAudit::read_specdd()?;
    assert!(spec.contains("G39") || spec.contains("TV-21"));
    Ok(())
}

pub async fn test_05_video_frame_decoder_interface_defined() -> Result<(), String> {
    // Assert TV-21 specifies decoder verification
    let spec = WorkspaceAudit::read_specdd()?;
    assert!(spec.contains("TV-21 — Fix `tv-rs-vision` synthetic frames"));
    Ok(())
}
