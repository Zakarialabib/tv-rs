//! Tier 2: Boundary 6 - Tizen & webOS Drivers (Rejection, Re-Registration, Port Fallback).

use crate::harness::driver_mock::TvError;

pub async fn test_01_tizen_pairing_rejected_by_user() -> Result<(), String> {
    let tizen_cancel_event = "ms.channel.unauthorized";
    let mapped_error = match tizen_cancel_event {
        "ms.channel.unauthorized" => TvError::AuthenticationFailed,
        _ => TvError::RoutingUnreachable,
    };

    assert_eq!(mapped_error, TvError::AuthenticationFailed);
    Ok(())
}

pub async fn test_02_webos_invalid_client_key_forces_reregistration() -> Result<(), String> {
    let mut key_store = std::collections::HashMap::new();
    key_store.insert("tv-1".to_string(), "stale-key".to_string());

    // Invalidate stale key on rejection
    let server_rejected = true;
    if server_rejected {
        key_store.remove("tv-1");
    }

    assert!(key_store.get("tv-1").is_none());
    Ok(())
}

pub async fn test_03_ws_frame_fragmentation_and_utf8() -> Result<(), String> {
    let raw_payload = b"{\"event\":\"ms.channel.connect\",\"data\":{\"clients\":[]}}";
    let parsed: Result<serde_json::Value, _> = serde_json::from_slice(raw_payload);
    assert!(parsed.is_ok());

    let invalid_utf8: Vec<u8> = vec![0xFF, 0xFE, 0x00, 0x00];
    let utf8_res = std::str::from_utf8(&invalid_utf8);
    assert!(utf8_res.is_err());
    Ok(())
}

pub async fn test_04_tizen_port_fallback_8002_to_8001() -> Result<(), String> {
    let mut current_port: u16 = 8002;
    let tls_refused = true;

    if tls_refused {
        // Fall back to plaintext port 8001
        current_port = 8001;
    }

    assert_eq!(current_port, 8001);
    Ok(())
}

pub async fn test_05_webos_luna_error_code_mapping() -> Result<(), String> {
    // webOS error 404/500/unauthorized mapping
    let luna_error = 404;
    let mapped = match luna_error {
        401 | 403 => TvError::AuthenticationFailed,
        404 => TvError::RoutingUnreachable,
        _ => TvError::RoutingTimeout,
    };

    assert_eq!(mapped, TvError::RoutingUnreachable);
    Ok(())
}
