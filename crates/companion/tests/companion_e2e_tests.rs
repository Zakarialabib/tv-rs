//! End-to-end tests for tv-rs-companion and tv-rs-provision:
//! - iOS TestFlight pairs on filtered network (multicast blocked/entitlement absent, SAS ceremony over direct QR handover).
//! - Android pairs (acquiring MulticastLock, mDNS/SSDP discovery) and executes full tv-rs conformance suite against DeviceHandle.

use tv_rs_companion::control::CompanionInput;
use tv_rs_companion::discovery::{DiscoveryGate, DiscoveryMode};
use tv_rs_companion::handle::CompanionDevice;
use tv_rs_companion::remote::RemoteKey;
use tv_rs_companion::uniffi_types::CompanionEngine;
use tv_rs_core::command::CommandId;
use tv_rs_core::conformance;
use tv_rs_core::{DeviceHandle, Key, PowerState};
use tv_rs_provision::improv::{ImprovCommand, ImprovSession, ImprovState};
use tv_rs_provision::multicast::MobilePlatform;
use tv_rs_provision::qr::QrPairingPayload;

#[test]
fn testflight_ios_pairs_on_filtered_network() {
    // 1. In iOS TestFlight build on a filtered guest / enterprise Wi-Fi where multicast
    // UDP traffic is dropped or multicast entitlement is restricted:
    let mut engine = CompanionEngine::new(MobilePlatform::Ios, "testflight-iphone-14");
    // iOS entitlement not granted / network filtered:
    engine.set_ios_multicast_entitlement(false);

    let gate = DiscoveryGate::new(MobilePlatform::Ios);
    assert_eq!(gate.select_discovery_mode(), DiscoveryMode::DirectFallback);

    // Filtered discovery uses known host fallback
    let known_hosts = vec!["10.200.1.42".to_string()];
    let discovered = engine
        .discover_displays(&known_hosts)
        .expect("Filtered network fallback discovery");
    assert_eq!(discovered.len(), 1);
    assert_eq!(discovered[0].host, "10.200.1.42");

    // 2. Display generates dynamic QR pairing payload
    let qr_payload = QrPairingPayload {
        display_id: "filtered-display-1".into(),
        display_name: "Boardroom Filtered Display".into(),
        ip_addresses: vec!["10.200.1.42".into()],
        port: 8080,
        auth_token: "testflight_secret_token_777".into(),
        expires_at: 2000000000,
    };
    let uri = qr_payload.to_uri();

    // 3. Companion scans QR URI, derives SAS matching the testflight client
    let expected_sas = qr_payload.compute_sas("testflight-iphone-14");
    let sas = engine.ingest_qr_uri(&uri).expect("Ingest QR URI");
    assert_eq!(sas, expected_sas);

    // 4. User verifies SAS code on screen and confirms pairing
    let session_token = engine
        .confirm_sas(&sas, "10.200.1.42", 8080)
        .expect("Confirm SAS code");
    assert!(session_token.starts_with("testflight-iphone-14:"));

    // 5. Commands dispatched on filtered network to tv-rs-hostd
    engine
        .send_key(RemoteKey::Home)
        .expect("Keypress dispatched");
    engine.send_power(true).expect("Power on dispatched");
    engine
        .send_input(CompanionInput::Hdmi(2))
        .expect("Input switch dispatched");
}

#[tokio::test]
async fn android_pairs_and_runs_conformance_suite() {
    // 1. First-boot BLE Improv Wi-Fi setup
    let mut improv = ImprovSession::new();
    let rpc_res = improv
        .handle_command(ImprovCommand::SendWifiCredentials {
            ssid: "Corporate-Display-Net".into(),
            password: "Wpa2SecurePassword".into(),
        })
        .expect("BLE Improv first-boot setup");
    assert_eq!(improv.state(), ImprovState::Provisioned);
    assert!(!rpc_res.values.is_empty());

    // 2. Android takes multicast lock
    let mut engine = CompanionEngine::new(MobilePlatform::Android, "android-pixel-tablet");
    engine.set_android_multicast_lock(true);

    let gate = DiscoveryGate::new(MobilePlatform::Android).with_android_lock(true);
    assert_eq!(gate.select_discovery_mode(), DiscoveryMode::MulticastLocal);

    let discovered = engine.discover_displays(&[]).expect("Multicast discovery");
    assert!(!discovered.is_empty());

    // 3. QR + SAS ceremony
    let qr_payload = QrPairingPayload {
        display_id: "android-target-display".into(),
        display_name: "Corporate Display".into(),
        ip_addresses: vec!["192.168.1.150".into()],
        port: 8080,
        auth_token: "improv_auth_token_888".into(),
        expires_at: 2000000000,
    };
    let uri = qr_payload.to_uri();
    let sas = engine.ingest_qr_uri(&uri).expect("Ingest QR URI");
    let session_token = engine
        .confirm_sas(&sas, "192.168.1.150", 8080)
        .expect("Confirm SAS");

    // 4. Wrap paired connection into DeviceHandle
    let device = CompanionDevice::new("android-target-display", "192.168.1.150", 8080);
    device.set_session_token(session_token);

    // 5. Run full tv-rs conformance suite against device
    let violations = conformance::run(&device).await;
    assert!(
        violations.is_empty(),
        "DeviceHandle must pass conformance suite: {violations:#?}"
    );

    // 6. Test basic functional operations
    let power = device.as_power().expect("PowerControl present");
    assert_eq!(power.power_state().await.unwrap(), PowerState::On);
    power.sleep().await.unwrap();
    assert_eq!(power.power_state().await.unwrap(), PowerState::Standby);
    power.wake().await.unwrap();
    assert_eq!(power.power_state().await.unwrap(), PowerState::On);

    let key = device.as_key().expect("KeyInput present");
    key.key_press(Key::Select, CommandId::new()).await.unwrap();
}
