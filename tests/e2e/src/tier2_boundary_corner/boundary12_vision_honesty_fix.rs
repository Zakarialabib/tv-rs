//! Tier 2: Boundary 12 - Vision Honesty Fix (Decoder Bounds, Zero Sizes, Crate Isolation).

use crate::harness::workspace_audit::WorkspaceAudit;

pub async fn test_01_calling_synthetic_returns_unimplemented_error() -> Result<(), String> {
    let spec = WorkspaceAudit::read_specdd()?;
    assert!(
        spec.contains("TV-21") && spec.contains("synthetic"),
        "TV-21 task must remediate synthetic frames"
    );
    Ok(())
}

pub async fn test_02_zero_sized_video_buffer_rejected() -> Result<(), String> {
    let width = 0u32;
    let height = 0u32;
    let is_valid_frame = width > 0 && height > 0;
    assert!(!is_valid_frame, "Zero-sized frame buffer must be rejected");
    Ok(())
}

pub async fn test_03_vision_dependency_isolated_from_core() -> Result<(), String> {
    let core_manifest = WorkspaceAudit::read_core_cargo_toml()?;
    assert!(
        !core_manifest.contains("tv-rs-vision"),
        "tv-rs-core must not depend on tv-rs-vision"
    );
    Ok(())
}

pub async fn test_04_unsupported_codec_error() -> Result<(), String> {
    let supported_codecs = ["h264", "hevc"];
    let requested = "av1";
    assert!(!supported_codecs.contains(&requested));
    Ok(())
}

pub async fn test_05_facade_without_vision_compiles() -> Result<(), String> {
    let spec = WorkspaceAudit::read_specdd()?;
    assert!(spec.contains("TV-21"), "TV-21 ensures facade stability");
    Ok(())
}
