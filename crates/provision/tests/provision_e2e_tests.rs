//! End-to-end tests for `tv-rs-provision`.

use tv_rs_provision::improv::{ImprovCommand, ImprovSession, ImprovState};
use tv_rs_provision::multicast::{MobilePlatform, MulticastLockGuard};
use tv_rs_provision::qr::QrPairingPayload;

#[test]
fn test_ble_improv_first_boot_then_qr_pairing() {
    // 1. First boot setup over BLE Improv
    let mut session = ImprovSession::new();
    let res = session
        .handle_command(ImprovCommand::SendWifiCredentials {
            ssid: "Corporate-Display-AP".into(),
            password: "SecureDisplayPassword123".into(),
        })
        .expect("Provision Wi-Fi");

    assert_eq!(session.state(), ImprovState::Provisioned);
    assert_eq!(session.provisioned_ssid(), Some("Corporate-Display-AP"));
    assert!(!res.values.is_empty());

    // 2. Multicast entitlement verification on iOS
    let ios_guard = MulticastLockGuard::new(MobilePlatform::Ios).with_ios_entitlement(true);
    assert!(ios_guard.verify_multicast_allowed().is_ok());

    // 3. QR pairing payload generation and SAS check
    let payload = QrPairingPayload {
        display_id: "disp-first-boot".into(),
        display_name: "Lobby Screen".into(),
        ip_addresses: vec!["192.168.1.100".into()],
        port: 8080,
        auth_token: "first_boot_token_abc".into(),
        expires_at: 1900000000,
    };
    let uri = payload.to_uri();
    let parsed = QrPairingPayload::from_uri(&uri).expect("Parse pairing URI");
    assert_eq!(parsed.display_id, "disp-first-boot");

    let sas = parsed.compute_sas("mobile-client-1");
    assert_eq!(sas.len(), 6);
}
