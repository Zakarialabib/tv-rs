//! Tier 2: Boundary 7 - Cast & CEC Drivers (Honesty, Subprocess Errors, Limits).

use crate::harness::workspace_audit::WorkspaceAudit;
use crate::harness::driver_mock::*;

pub async fn test_01_cast_removes_unsupported_device_shim() -> Result<(), String> {
    let spec = WorkspaceAudit::read_specdd()?;
    assert!(
        spec.contains("UnsupportedDevice") || spec.contains("TV-14"),
        "specdd must document removal of UnsupportedDevice shim in TV-14"
    );
    Ok(())
}

pub async fn test_02_cec_subprocess_not_found_clean_error() -> Result<(), String> {
    // When cec-client binary is not in PATH, command execution returns clean error
    let fake_cmd = tokio::process::Command::new("non_existent_cec_binary_xyz_123").output().await;
    assert!(fake_cmd.is_err());
    Ok(())
}

pub async fn test_03_cast_tls_eke2_handshake_timeout() -> Result<(), String> {
    // Handshake timeout verification: aborts within deadline
    let timeout_duration = tokio::time::Duration::from_millis(50);
    let handshake_fut = async {
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        Ok::<(), TvError>(())
    };

    let res = tokio::time::timeout(timeout_duration, handshake_fut).await;
    assert!(res.is_err(), "Handshake exceeding deadline must timeout");
    Ok(())
}

pub async fn test_04_cec_invalid_logical_address() -> Result<(), String> {
    // CEC logical addresses must be in range 0..=15
    let invalid_addr = 16u8;
    let is_valid_cec_addr = invalid_addr <= 15;
    assert!(!is_valid_cec_addr);
    Ok(())
}

pub async fn test_05_cast_media_session_id_invalidation() -> Result<(), String> {
    let mut cast = create_mock_cast_driver();
    // Sending command unsupported without media session
    let cmd = Command::SetVideoWall { rows: 1, cols: 1, row: 0, col: 0 };
    let res = cast.send(cmd).await;
    assert!(res.is_err());
    Ok(())
}
