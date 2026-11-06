//! Tests for the capability matrix, Hello updates, and companion app constraints.

use tv_rs_core::caps::{
    CapabilityId, CapabilityMatrix, EndpointCapabilities, HelloUpdate, MulticastLockRequired,
    PermissionStatus, UiTile,
};
use tv_rs_core::descriptor::PowerState;
use tv_rs_core::error::Remedy;

#[test]
fn test_exit_criterion_roku_no_input_linux_host_no_uinput_phone_shows_no_remote_tile() {
    // Exit Criterion: Roku (no input) through Linux host (no uinput) -> phone shows no Remote tile
    let mut matrix = CapabilityMatrix::new();

    // Roku endpoint without direct remote keys capability (e.g. display / headless):
    matrix.tv = EndpointCapabilities::new()
        .with_capability(CapabilityId::TV_POWER)
        .with_capability(CapabilityId::TV_APP_LAUNCH);

    // Linux host node mediating control, without uinput injection support:
    matrix.host = EndpointCapabilities::new()
        .with_capability(CapabilityId::HOST_SERIAL)
        .with_capability(CapabilityId::HOST_DDCCI);

    // Phone companion app:
    matrix.phone = EndpointCapabilities::new().with_capability(CapabilityId::PHONE_LOCAL_NETWORK);

    let tiles = matrix.project_ui_tiles();

    // Verify remote tile is absent
    assert!(!tiles.contains(&UiTile::Remote));
    // Verify other tiles like Power and AppLauncher still project correctly
    assert!(tiles.contains(&UiTile::Power));
    assert!(tiles.contains(&UiTile::AppLauncher));

    // When the Linux host gains uinput capability (e.g. /dev/uinput permissions granted):
    matrix.host = matrix.host.with_capability(CapabilityId::HOST_UINPUT);
    let tiles_updated = matrix.project_ui_tiles();
    assert!(tiles_updated.contains(&UiTile::Remote));
}

#[test]
fn test_exit_criterion_phone_in_pairing_mode_without_lock_typed_multicast_lock_required() {
    // Exit Criterion: phone in pairing mode without lock -> typed MulticastLockRequired with one-tap fix
    let mut matrix = CapabilityMatrix::new();

    // Phone enters pairing mode:
    matrix.phone = EndpointCapabilities::new()
        .with_capability(CapabilityId::PHONE_PAIRING)
        .with_permission("ACCESS_LOCAL_NETWORK", PermissionStatus::Granted);

    let result = matrix.check_phone_pairing_multicast();
    let err = result.expect_err("Must fail when multicast lock is missing in pairing mode");

    // Must be typed MulticastLockRequired:
    let multicast_err: MulticastLockRequired = err;
    assert_eq!(multicast_err.one_tap_action, "acquire_multicast_lock");
    assert_eq!(multicast_err.remedy(), Remedy::RequestPermission);
    assert!(multicast_err
        .hint()
        .contains("Multicast packet reception requires a held Wi-Fi MulticastLock"));
    assert!(multicast_err
        .hint()
        .contains("Tap 'acquire_multicast_lock' to enable."));

    // Once one-tap fix executes and lock is held:
    matrix.phone = matrix
        .phone
        .with_capability(CapabilityId::PHONE_MULTICAST_LOCK);
    assert!(matrix.check_phone_pairing_multicast().is_ok());
}

#[test]
fn test_hello_update_reactive_flow() {
    let mut matrix = CapabilityMatrix::new();
    matrix.power_state = Some(PowerState::Off);
    matrix.tv = EndpointCapabilities::new().with_capability(CapabilityId::TV_POWER);

    // Simulate power-level update and permission grant:
    matrix.power_state = Some(PowerState::On);
    let hello = HelloUpdate::new(matrix)
        .with_permission_delta("ACCESS_LOCAL_NETWORK", PermissionStatus::Granted)
        .with_addon_delta("hdr_plus", true);

    assert_eq!(hello.power_state, Some(PowerState::On));
    assert_eq!(hello.permissions_delta.len(), 1);
    assert_eq!(hello.permissions_delta[0].0, "ACCESS_LOCAL_NETWORK");
    assert_eq!(hello.permissions_delta[0].1, PermissionStatus::Granted);
    assert_eq!(hello.addons_delta[0], ("hdr_plus".to_string(), true));
}
