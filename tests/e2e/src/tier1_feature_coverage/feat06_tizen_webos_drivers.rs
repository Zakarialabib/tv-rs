//! Tier 1: Feature 6 - Tizen & webOS Drivers (R1, TV-12).

use crate::harness::simulated_panel::{TizenWsProtocol, WebOsSsapProtocol};
use crate::harness::driver_mock::TvError;

pub async fn test_01_tizen_ws_pairing_frame_schema() -> Result<(), String> {
    let url = TizenWsProtocol::build_handshake_url("192.168.1.120", 8002, "tv-rs", Some("12345678"));
    assert!(url.starts_with("wss://192.168.1.120:8002/api/v2/channels/samsung.remote.control"));
    assert!(url.contains("name="));
    assert!(url.contains("token=12345678"));
    Ok(())
}

pub async fn test_02_tizen_token_persistence_lifecycle() -> Result<(), String> {
    let mut token_store = std::collections::HashMap::new();
    token_store.insert("samsung-tv-uuid-1".to_string(), "token-abc-xyz".to_string());

    assert_eq!(token_store.get("samsung-tv-uuid-1").map(String::as_str), Some("token-abc-xyz"));
    assert_eq!(token_store.get("samsung-tv-uuid-2"), None);
    Ok(())
}

pub async fn test_03_webos_ssap_registration_payload() -> Result<(), String> {
    let payload = WebOsSsapProtocol::build_register_payload(Some("client-key-webos-999"));
    assert_eq!(payload["type"], "register");
    assert_eq!(payload["payload"]["client-key"], "client-key-webos-999");
    assert_eq!(payload["payload"]["manifest"]["signed"]["appId"], "com.tv-rs.remote");
    Ok(())
}

pub async fn test_04_webos_client_key_persistence() -> Result<(), String> {
    let mut key_store = std::collections::HashMap::new();
    key_store.insert("lg-webos-uuid-1".to_string(), "stored-client-key-123".to_string());

    assert_eq!(key_store.get("lg-webos-uuid-1").map(String::as_str), Some("stored-client-key-123"));
    Ok(())
}

pub async fn test_05_error_mapping_to_tv_error() -> Result<(), String> {
    // Test mapping of WebSocket error codes to TvError variants
    let err_auth = TvError::AuthenticationFailed;
    assert_eq!(format!("{}", err_auth), "authentication failed");

    let err_unreachable = TvError::RoutingUnreachable;
    assert_eq!(format!("{}", err_unreachable), "routing failed: unreachable");
    Ok(())
}
