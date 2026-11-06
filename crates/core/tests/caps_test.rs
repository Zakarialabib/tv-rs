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

/// Simple deterministic pseudo-random number generator (xorshift64star)
/// for property-based testing without external dependencies.
struct SimpleRng {
    state: u64,
}

impl SimpleRng {
    fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 0xdeadbeefcafe } else { seed },
        }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }

    fn gen_bool(&mut self) -> bool {
        (self.next_u64() & 1) == 1
    }

    fn gen_range(&mut self, max: usize) -> usize {
        (self.next_u64() % (max as u64)) as usize
    }
}

/// Property-based tests generating random capability sets and asserting that
/// the projected UI never offers an action the device cannot fulfill.
///
/// Mathematical invariant (Fail-fast guarantee):
/// - Tile present => the underlying capability prerequisites are strictly satisfied.
/// - Tile absent => the underlying capability prerequisites are NOT satisfied.
#[test]
fn property_test_ui_tile_projection_never_offers_unfulfillable_action() {
    let all_capabilities = [
        CapabilityId::TV_POWER,
        CapabilityId::TV_INPUT_KEYS,
        CapabilityId::TV_INPUT_TEXT,
        CapabilityId::TV_APP_LAUNCH,
        CapabilityId::TV_VOLUME,
        CapabilityId::TV_INPUT_SELECT,
        CapabilityId::HOST_UINPUT,
        CapabilityId::HOST_DDCCI,
        CapabilityId::HOST_CEC,
        CapabilityId::HOST_SERIAL,
        CapabilityId::PHONE_MULTICAST_LOCK,
        CapabilityId::PHONE_LOCAL_NETWORK,
        CapabilityId::PHONE_PAIRING,
        "tv.custom.signal",
        "host.custom.daemon",
        "phone.custom.addon",
    ];

    let permissions = [
        "ACCESS_LOCAL_NETWORK",
        "MULTICAST_LOCK",
        "CAMERA",
        "BLUETOOTH_CONNECT",
    ];

    let addons = [
        "pairing_mode",
        "hdr_plus",
        "multi_window",
        "audio_passthrough",
    ];

    let mut rng = SimpleRng::new(0x1337_cafe_beef);
    const NUM_ITERATIONS: usize = 10_000;

    for iteration in 0..NUM_ITERATIONS {
        let mut matrix = CapabilityMatrix::new();

        // 1. Generate TV capabilities
        for cap in &all_capabilities {
            if rng.gen_bool() {
                matrix.tv = matrix.tv.clone().with_capability(*cap);
            }
        }
        for perm in &permissions {
            if rng.gen_bool() {
                let status = match rng.gen_range(4) {
                    0 => PermissionStatus::Granted,
                    1 => PermissionStatus::PromptNeeded,
                    2 => PermissionStatus::Denied,
                    _ => PermissionStatus::Blocked,
                };
                matrix.tv = matrix.tv.clone().with_permission(*perm, status);
            }
        }
        for addon in &addons {
            if rng.gen_bool() {
                matrix.tv = matrix.tv.clone().with_addon(*addon);
            }
        }

        // 2. Generate Host capabilities
        for cap in &all_capabilities {
            if rng.gen_bool() {
                matrix.host = matrix.host.clone().with_capability(*cap);
            }
        }
        for perm in &permissions {
            if rng.gen_bool() {
                let status = match rng.gen_range(4) {
                    0 => PermissionStatus::Granted,
                    1 => PermissionStatus::PromptNeeded,
                    2 => PermissionStatus::Denied,
                    _ => PermissionStatus::Blocked,
                };
                matrix.host = matrix.host.clone().with_permission(*perm, status);
            }
        }
        for addon in &addons {
            if rng.gen_bool() {
                matrix.host = matrix.host.clone().with_addon(*addon);
            }
        }

        // 3. Generate Phone capabilities
        for cap in &all_capabilities {
            if rng.gen_bool() {
                matrix.phone = matrix.phone.clone().with_capability(*cap);
            }
        }
        for perm in &permissions {
            if rng.gen_bool() {
                let status = match rng.gen_range(4) {
                    0 => PermissionStatus::Granted,
                    1 => PermissionStatus::PromptNeeded,
                    2 => PermissionStatus::Denied,
                    _ => PermissionStatus::Blocked,
                };
                matrix.phone = matrix.phone.clone().with_permission(*perm, status);
            }
        }
        for addon in &addons {
            if rng.gen_bool() {
                matrix.phone = matrix.phone.clone().with_addon(*addon);
            }
        }

        let projected_tiles = matrix.project_ui_tiles();

        // --- Mathematical Invariant Proof: Fail-Fast Soundness ---

        // Invariant 1: Power tile
        let can_fulfill_power = matrix.tv.contains(CapabilityId::TV_POWER);
        let has_power_tile = projected_tiles.contains(&UiTile::Power);
        assert_eq!(
            has_power_tile, can_fulfill_power,
            "Iteration {iteration}: Power tile offered iff TV has tv.power. Has tile: {has_power_tile}, can fulfill: {can_fulfill_power}"
        );

        // Invariant 2: Remote tile
        let tv_has_input = matrix.tv.contains(CapabilityId::TV_INPUT_KEYS);
        let host_has_uinput = matrix.host.contains(CapabilityId::HOST_UINPUT);
        let host_is_present =
            !matrix.host.capabilities.is_empty() || !matrix.host.addons.is_empty();
        let can_fulfill_remote = if host_is_present {
            tv_has_input || host_has_uinput
        } else {
            tv_has_input
        };
        let has_remote_tile = projected_tiles.contains(&UiTile::Remote);
        assert_eq!(
            has_remote_tile, can_fulfill_remote,
            "Iteration {iteration}: Remote tile offered iff remote can be fulfilled. Has tile: {has_remote_tile}, can fulfill: {can_fulfill_remote}"
        );

        // Invariant 3: Volume tile
        let can_fulfill_volume = matrix.tv.contains(CapabilityId::TV_VOLUME);
        let has_volume_tile = projected_tiles.contains(&UiTile::Volume);
        assert_eq!(
            has_volume_tile, can_fulfill_volume,
            "Iteration {iteration}: Volume tile offered iff TV has tv.volume. Has tile: {has_volume_tile}, can fulfill: {can_fulfill_volume}"
        );

        // Invariant 4: AppLauncher tile
        let can_fulfill_app_launch = matrix.tv.contains(CapabilityId::TV_APP_LAUNCH);
        let has_app_launcher_tile = projected_tiles.contains(&UiTile::AppLauncher);
        assert_eq!(
            has_app_launcher_tile, can_fulfill_app_launch,
            "Iteration {iteration}: AppLauncher tile offered iff TV has tv.app.launch. Has tile: {has_app_launcher_tile}, can fulfill: {can_fulfill_app_launch}"
        );

        // Invariant 5: InputSource tile
        let can_fulfill_input_select = matrix.tv.contains(CapabilityId::TV_INPUT_SELECT);
        let has_input_source_tile = projected_tiles.contains(&UiTile::InputSource);
        assert_eq!(
            has_input_source_tile, can_fulfill_input_select,
            "Iteration {iteration}: InputSource tile offered iff TV has tv.input.select. Has tile: {has_input_source_tile}, can fulfill: {can_fulfill_input_select}"
        );

        // Invariant 6: Pairing tile
        let can_fulfill_pairing = matrix.phone.contains(CapabilityId::PHONE_PAIRING)
            || matrix.phone.addons.contains("pairing_mode");
        let has_pairing_tile = projected_tiles.contains(&UiTile::Pairing);
        assert_eq!(
            has_pairing_tile, can_fulfill_pairing,
            "Iteration {iteration}: Pairing tile offered iff Phone has phone.pairing or pairing_mode addon. Has tile: {has_pairing_tile}, can fulfill: {can_fulfill_pairing}"
        );

        // Invariant 7: Multicast lock check fail-fast guarantee
        let pairing_mode = matrix.phone.contains(CapabilityId::PHONE_PAIRING)
            || matrix.phone.addons.contains("pairing_mode");
        let has_multicast_lock = matrix.phone.contains(CapabilityId::PHONE_MULTICAST_LOCK);
        let lock_check = matrix.check_phone_pairing_multicast();
        if pairing_mode && !has_multicast_lock {
            assert!(
                lock_check.is_err(),
                "Iteration {iteration}: Pairing without multicast lock must return MulticastLockRequired"
            );
        } else {
            assert!(
                lock_check.is_ok(),
                "Iteration {iteration}: Pairing with multicast lock or inactive pairing must succeed"
            );
        }
    }
}
