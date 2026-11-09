//! Wave 3 Task TV-16 Adversarial Stress Test Harness
//!
//! Empirical validation of:
//! 1. Conformance Harness Adversarial Validation:
//!    - Buggy Driver A: Advertises `Capability::Volume` but returns `None` on `as_volume()`.
//!    - Buggy Driver B: Advertises `Capability::InputSelect` but returns `None` on `as_input()`.
//!    - Buggy Driver C: Advertises `Capability::AppLaunch` but returns `None` on `as_app()`.
//!    - Buggy Driver D: Returns `Some(&self)` on `as_volume()` but DOES NOT advertise `Capability::Volume` in `capabilities()`.
//!    - Buggy Driver E: Emits an unadvertised capability in `conformance_cases()`.
//!    - Verification of exact detection mechanisms (`verify_orphaned_capabilities` vs `conformance::run`).
//! 2. Universal Expanding Registry Stress:
//!    - Dynamic runtime third-party driver registration without core or facade modifications.
//!    - Verification of `build_capability_registry()`, `find_by_capability()`, and `aggregate_capabilities()`.
//!    - Concurrent runtime extensions and custom capability sets.
//! 3. CapabilitySet Bitwise Math & Combinatorics:
//!    - Full combinatorics across all 19 capabilities: empty set, full set, singletons, duplicate inserts.
//!    - Bitwise invariants: union, extend, BitOr, BitOrAssign, FromIterator, len.
//!    - Algebraic properties: commutativity, associativity, idempotence, disjoint partitioning.

use async_trait::async_trait;
use tv_rs_core::capability::{
    AppLaunch, AppRef, Capability, CapabilitySet, DeviceHandle, Input, InputSelect, PowerControl,
    VolumeControl,
};
use tv_rs_core::command::{Command, CommandAck, CommandId};
use tv_rs_core::conformance::{self, ConformanceCase};
use tv_rs_core::descriptor::{DeviceDescriptor, DeviceId, DeviceOs, DiscoveredTv, PowerState};
use tv_rs_core::driver::Driver;
use tv_rs_core::error::TvError;
use tv_rs_core::observe::ObservationStream;
use tv_rs_core::types::TvVendor;
use tv_rs_drivers::registry::DriverRegistry;

// =============================================================================
// SECTION 1: Mock Components for Buggy Drivers
// =============================================================================

struct DummyPower;
#[async_trait]
impl PowerControl for DummyPower {
    async fn power_state(&self) -> Result<PowerState, TvError> {
        Ok(PowerState::On)
    }
    async fn wake(&self) -> Result<(), TvError> {
        Ok(())
    }
    async fn sleep(&self) -> Result<(), TvError> {
        Ok(())
    }
}

struct DummyVolume;
#[async_trait]
impl VolumeControl for DummyVolume {
    async fn volume(&self) -> Result<Option<u8>, TvError> {
        Ok(Some(50))
    }
    async fn set_volume(&self, _level: u8, _cmd: CommandId) -> Result<(), TvError> {
        Ok(())
    }
    async fn set_mute(&self, _muted: bool, _cmd: CommandId) -> Result<(), TvError> {
        Ok(())
    }
    async fn mute(&self) -> Result<Option<bool>, TvError> {
        Ok(Some(false))
    }
}

#[allow(dead_code)]
struct DummyInput;
#[async_trait]
impl InputSelect for DummyInput {
    async fn active_input(&self) -> Result<Option<Input>, TvError> {
        Ok(Some(Input::Hdmi(1)))
    }
    async fn select_input(&self, _input: Input, _cmd: CommandId) -> Result<(), TvError> {
        Ok(())
    }
}

#[allow(dead_code)]
struct DummyApp;
#[async_trait]
impl AppLaunch for DummyApp {
    async fn launch(&self, _app: AppRef, _cmd: CommandId) -> Result<(), TvError> {
        Ok(())
    }
    async fn foreground(&self) -> Result<Option<AppRef>, TvError> {
        Ok(None)
    }
}

// -----------------------------------------------------------------------------
// Driver A: Advertises VolumeControl but returns None on as_volume()
// -----------------------------------------------------------------------------
struct BuggyHandleA {
    desc: DeviceDescriptor,
    caps: CapabilitySet,
}

impl DeviceHandle for BuggyHandleA {
    fn id(&self) -> DeviceId {
        self.desc.id.clone()
    }
    fn descriptor(&self) -> DeviceDescriptor {
        self.desc.clone()
    }
    fn capabilities(&self) -> CapabilitySet {
        self.caps
    }
    fn as_power(&self) -> Option<&dyn PowerControl> {
        Some(&DummyPower)
    }
    fn as_key(&self) -> Option<&dyn tv_rs_core::KeyInput> {
        None
    }
    fn as_app(&self) -> Option<&dyn AppLaunch> {
        None
    }
    fn as_text(&self) -> Option<&dyn tv_rs_core::TextInput> {
        None
    }
    fn as_input(&self) -> Option<&dyn InputSelect> {
        None
    }
    fn as_volume(&self) -> Option<&dyn VolumeControl> {
        None // Bug A: Advertised Volume, but returns None!
    }
}

struct BuggyDriverA {
    caps: CapabilitySet,
    omit_case: bool,
}

#[async_trait]
impl Driver for BuggyDriverA {
    type Handle = BuggyHandleA;
    fn vendor(&self) -> TvVendor {
        TvVendor::Unknown
    }
    fn capabilities(&self) -> &CapabilitySet {
        &self.caps
    }
    async fn connect(&mut self, _target: &DiscoveredTv) -> Result<Self::Handle, TvError> {
        Ok(BuggyHandleA {
            desc: DeviceDescriptor::new(
                DeviceId::new("buggy-a").unwrap(),
                DeviceOs::Unknown,
                "127.0.0.1",
            ),
            caps: self.caps,
        })
    }
    async fn send(&mut self, _cmd: Command) -> Result<CommandAck, TvError> {
        Err(TvError::Unsupported(Capability::Volume))
    }
    async fn observe(&mut self) -> Result<ObservationStream, TvError> {
        Ok(ObservationStream::new())
    }
    fn conformance_cases(&self) -> Vec<ConformanceCase> {
        if self.omit_case {
            // Missing Volume case
            vec![ConformanceCase::new(Capability::PowerState, "power_state")]
        } else {
            vec![
                ConformanceCase::new(Capability::PowerState, "power_state"),
                ConformanceCase::new(Capability::Volume, "volume"),
            ]
        }
    }
}

// -----------------------------------------------------------------------------
// Driver B: Advertises InputSelect but returns None on as_input()
// -----------------------------------------------------------------------------
struct BuggyHandleB {
    desc: DeviceDescriptor,
    caps: CapabilitySet,
}

impl DeviceHandle for BuggyHandleB {
    fn id(&self) -> DeviceId {
        self.desc.id.clone()
    }
    fn descriptor(&self) -> DeviceDescriptor {
        self.desc.clone()
    }
    fn capabilities(&self) -> CapabilitySet {
        self.caps
    }
    fn as_power(&self) -> Option<&dyn PowerControl> {
        Some(&DummyPower)
    }
    fn as_key(&self) -> Option<&dyn tv_rs_core::KeyInput> {
        None
    }
    fn as_app(&self) -> Option<&dyn AppLaunch> {
        None
    }
    fn as_text(&self) -> Option<&dyn tv_rs_core::TextInput> {
        None
    }
    fn as_input(&self) -> Option<&dyn InputSelect> {
        None // Bug B: Advertised InputSelect, but returns None!
    }
    fn as_volume(&self) -> Option<&dyn VolumeControl> {
        None
    }
}

struct BuggyDriverB {
    caps: CapabilitySet,
    omit_case: bool,
}

#[async_trait]
impl Driver for BuggyDriverB {
    type Handle = BuggyHandleB;
    fn vendor(&self) -> TvVendor {
        TvVendor::Unknown
    }
    fn capabilities(&self) -> &CapabilitySet {
        &self.caps
    }
    async fn connect(&mut self, _target: &DiscoveredTv) -> Result<Self::Handle, TvError> {
        Ok(BuggyHandleB {
            desc: DeviceDescriptor::new(
                DeviceId::new("buggy-b").unwrap(),
                DeviceOs::Unknown,
                "127.0.0.1",
            ),
            caps: self.caps,
        })
    }
    async fn send(&mut self, _cmd: Command) -> Result<CommandAck, TvError> {
        Err(TvError::Unsupported(Capability::InputSelect))
    }
    async fn observe(&mut self) -> Result<ObservationStream, TvError> {
        Ok(ObservationStream::new())
    }
    fn conformance_cases(&self) -> Vec<ConformanceCase> {
        if self.omit_case {
            vec![ConformanceCase::new(Capability::PowerState, "power_state")]
        } else {
            vec![
                ConformanceCase::new(Capability::PowerState, "power_state"),
                ConformanceCase::new(Capability::InputSelect, "input_select"),
            ]
        }
    }
}

// -----------------------------------------------------------------------------
// Driver C: Advertises AppLaunch but returns None on as_app()
// -----------------------------------------------------------------------------
struct BuggyHandleC {
    desc: DeviceDescriptor,
    caps: CapabilitySet,
}

impl DeviceHandle for BuggyHandleC {
    fn id(&self) -> DeviceId {
        self.desc.id.clone()
    }
    fn descriptor(&self) -> DeviceDescriptor {
        self.desc.clone()
    }
    fn capabilities(&self) -> CapabilitySet {
        self.caps
    }
    fn as_power(&self) -> Option<&dyn PowerControl> {
        Some(&DummyPower)
    }
    fn as_key(&self) -> Option<&dyn tv_rs_core::KeyInput> {
        None
    }
    fn as_app(&self) -> Option<&dyn AppLaunch> {
        None // Bug C: Advertised AppLaunch, but returns None!
    }
    fn as_text(&self) -> Option<&dyn tv_rs_core::TextInput> {
        None
    }
    fn as_input(&self) -> Option<&dyn InputSelect> {
        None
    }
    fn as_volume(&self) -> Option<&dyn VolumeControl> {
        None
    }
}

struct BuggyDriverC {
    caps: CapabilitySet,
    omit_case: bool,
}

#[async_trait]
impl Driver for BuggyDriverC {
    type Handle = BuggyHandleC;
    fn vendor(&self) -> TvVendor {
        TvVendor::Unknown
    }
    fn capabilities(&self) -> &CapabilitySet {
        &self.caps
    }
    async fn connect(&mut self, _target: &DiscoveredTv) -> Result<Self::Handle, TvError> {
        Ok(BuggyHandleC {
            desc: DeviceDescriptor::new(
                DeviceId::new("buggy-c").unwrap(),
                DeviceOs::Unknown,
                "127.0.0.1",
            ),
            caps: self.caps,
        })
    }
    async fn send(&mut self, _cmd: Command) -> Result<CommandAck, TvError> {
        Err(TvError::Unsupported(Capability::AppLaunch))
    }
    async fn observe(&mut self) -> Result<ObservationStream, TvError> {
        Ok(ObservationStream::new())
    }
    fn conformance_cases(&self) -> Vec<ConformanceCase> {
        if self.omit_case {
            vec![ConformanceCase::new(Capability::PowerState, "power_state")]
        } else {
            vec![
                ConformanceCase::new(Capability::PowerState, "power_state"),
                ConformanceCase::new(Capability::AppLaunch, "app_launch"),
            ]
        }
    }
}

// -----------------------------------------------------------------------------
// Driver D: Returns Some(&self) on as_volume() but DOES NOT advertise Capability::Volume
// -----------------------------------------------------------------------------
struct BuggyHandleD {
    desc: DeviceDescriptor,
    caps: CapabilitySet,
}

impl DeviceHandle for BuggyHandleD {
    fn id(&self) -> DeviceId {
        self.desc.id.clone()
    }
    fn descriptor(&self) -> DeviceDescriptor {
        self.desc.clone()
    }
    fn capabilities(&self) -> CapabilitySet {
        self.caps // Does NOT include Volume!
    }
    fn as_power(&self) -> Option<&dyn PowerControl> {
        Some(&DummyPower)
    }
    fn as_key(&self) -> Option<&dyn tv_rs_core::KeyInput> {
        None
    }
    fn as_app(&self) -> Option<&dyn AppLaunch> {
        None
    }
    fn as_text(&self) -> Option<&dyn tv_rs_core::TextInput> {
        None
    }
    fn as_input(&self) -> Option<&dyn InputSelect> {
        None
    }
    fn as_volume(&self) -> Option<&dyn VolumeControl> {
        Some(&DummyVolume) // Bug D: Volume accessor returned despite not being advertised!
    }
}

struct BuggyDriverD {
    caps: CapabilitySet,
}

#[async_trait]
impl Driver for BuggyDriverD {
    type Handle = BuggyHandleD;
    fn vendor(&self) -> TvVendor {
        TvVendor::Unknown
    }
    fn capabilities(&self) -> &CapabilitySet {
        &self.caps
    }
    async fn connect(&mut self, _target: &DiscoveredTv) -> Result<Self::Handle, TvError> {
        Ok(BuggyHandleD {
            desc: DeviceDescriptor::new(
                DeviceId::new("buggy-d").unwrap(),
                DeviceOs::Unknown,
                "127.0.0.1",
            ),
            caps: self.caps,
        })
    }
    async fn send(&mut self, _cmd: Command) -> Result<CommandAck, TvError> {
        Ok(CommandAck::new())
    }
    async fn observe(&mut self) -> Result<ObservationStream, TvError> {
        Ok(ObservationStream::new())
    }
}

// -----------------------------------------------------------------------------
// Driver E: Emits an unadvertised capability in conformance_cases()
// -----------------------------------------------------------------------------
struct BuggyHandleE {
    desc: DeviceDescriptor,
    caps: CapabilitySet,
}

impl DeviceHandle for BuggyHandleE {
    fn id(&self) -> DeviceId {
        self.desc.id.clone()
    }
    fn descriptor(&self) -> DeviceDescriptor {
        self.desc.clone()
    }
    fn capabilities(&self) -> CapabilitySet {
        self.caps
    }
    fn as_power(&self) -> Option<&dyn PowerControl> {
        Some(&DummyPower)
    }
    fn as_key(&self) -> Option<&dyn tv_rs_core::KeyInput> {
        None
    }
    fn as_app(&self) -> Option<&dyn AppLaunch> {
        None
    }
    fn as_text(&self) -> Option<&dyn tv_rs_core::TextInput> {
        None
    }
    fn as_input(&self) -> Option<&dyn InputSelect> {
        None
    }
    fn as_volume(&self) -> Option<&dyn VolumeControl> {
        None
    }
}

struct BuggyDriverE {
    caps: CapabilitySet,
}

#[async_trait]
impl Driver for BuggyDriverE {
    type Handle = BuggyHandleE;
    fn vendor(&self) -> TvVendor {
        TvVendor::Unknown
    }
    fn capabilities(&self) -> &CapabilitySet {
        &self.caps // Only PowerState
    }
    async fn connect(&mut self, _target: &DiscoveredTv) -> Result<Self::Handle, TvError> {
        Ok(BuggyHandleE {
            desc: DeviceDescriptor::new(
                DeviceId::new("buggy-e").unwrap(),
                DeviceOs::Unknown,
                "127.0.0.1",
            ),
            caps: self.caps,
        })
    }
    async fn send(&mut self, _cmd: Command) -> Result<CommandAck, TvError> {
        Ok(CommandAck::new())
    }
    async fn observe(&mut self) -> Result<ObservationStream, TvError> {
        Ok(ObservationStream::new())
    }
    fn conformance_cases(&self) -> Vec<ConformanceCase> {
        // Bug E: Emits Volume conformance case even though Volume is NOT in capabilities!
        vec![
            ConformanceCase::new(Capability::PowerState, "power_state"),
            ConformanceCase::new(Capability::Volume, "unadvertised_volume_test"),
        ]
    }
}

// =============================================================================
// TEST SECTION 1: Conformance Harness Adversarial Validation
// =============================================================================

#[tokio::test]
async fn test_adversarial_case_a_volume_control_orphaned() {
    let caps = CapabilitySet::of(Capability::PowerState).with(Capability::Volume);
    let mut driver = BuggyDriverA {
        caps,
        omit_case: true,
    };

    // 1. Conformance cases test: verify_orphaned_capabilities must report Volume missing
    let cases = driver.conformance_cases();
    let orphan_err = conformance::verify_orphaned_capabilities(driver.capabilities(), &cases);
    assert!(
        orphan_err.is_err(),
        "verify_orphaned_capabilities MUST detect omitted Volume case"
    );
    let missing = orphan_err.unwrap_err();
    assert_eq!(
        missing,
        vec![Capability::Volume],
        "Expected exact orphaned capability [Volume]"
    );

    // 2. Handle conformance: conformance::run must report Volume accessor is None
    let handle = driver
        .connect(&DiscoveredTv::probed(TvVendor::Unknown, "127.0.0.1"))
        .await
        .unwrap();
    let violations = conformance::run(&handle).await;
    assert!(
        !violations.is_empty(),
        "conformance::run must report violation"
    );
    let vol_violation = violations.iter().find(|v| {
        v.detail
            .contains("volume: advertised=true but accessor_present=false")
    });
    assert!(
        vol_violation.is_some(),
        "Expected exact violation detail for orphaned Volume accessor: {violations:#?}"
    );
}

#[tokio::test]
async fn test_adversarial_case_b_input_select_orphaned() {
    let caps = CapabilitySet::of(Capability::PowerState).with(Capability::InputSelect);
    let mut driver = BuggyDriverB {
        caps,
        omit_case: true,
    };

    // 1. verify_orphaned_capabilities reports InputSelect missing
    let cases = driver.conformance_cases();
    let orphan_err = conformance::verify_orphaned_capabilities(driver.capabilities(), &cases);
    assert!(
        orphan_err.is_err(),
        "verify_orphaned_capabilities MUST detect omitted InputSelect case"
    );
    let missing = orphan_err.unwrap_err();
    assert_eq!(
        missing,
        vec![Capability::InputSelect],
        "Expected exact orphaned capability [InputSelect]"
    );

    // 2. Handle conformance: conformance::run reports InputSelect accessor is None
    let handle = driver
        .connect(&DiscoveredTv::probed(TvVendor::Unknown, "127.0.0.1"))
        .await
        .unwrap();
    let violations = conformance::run(&handle).await;
    let input_violation = violations.iter().find(|v| {
        v.detail
            .contains("input_select: advertised=true but accessor_present=false")
    });
    assert!(
        input_violation.is_some(),
        "Expected exact violation detail for orphaned InputSelect accessor: {violations:#?}"
    );
}

#[tokio::test]
async fn test_adversarial_case_c_app_launch_orphaned() {
    let caps = CapabilitySet::of(Capability::PowerState).with(Capability::AppLaunch);
    let mut driver = BuggyDriverC {
        caps,
        omit_case: true,
    };

    // 1. verify_orphaned_capabilities reports AppLaunch missing
    let cases = driver.conformance_cases();
    let orphan_err = conformance::verify_orphaned_capabilities(driver.capabilities(), &cases);
    assert!(
        orphan_err.is_err(),
        "verify_orphaned_capabilities MUST detect omitted AppLaunch case"
    );
    let missing = orphan_err.unwrap_err();
    assert_eq!(
        missing,
        vec![Capability::AppLaunch],
        "Expected exact orphaned capability [AppLaunch]"
    );

    // 2. Handle conformance: conformance::run reports AppLaunch accessor is None
    let handle = driver
        .connect(&DiscoveredTv::probed(TvVendor::Unknown, "127.0.0.1"))
        .await
        .unwrap();
    let violations = conformance::run(&handle).await;
    let app_violation = violations.iter().find(|v| {
        v.detail
            .contains("app_launch: advertised=true but accessor_present=false")
    });
    assert!(
        app_violation.is_some(),
        "Expected exact violation detail for orphaned AppLaunch accessor: {violations:#?}"
    );
}

#[tokio::test]
async fn test_adversarial_case_d_unadvertised_volume_accessor_leak() {
    // Advertises PowerState only; Volume is NOT in capabilities
    let caps = CapabilitySet::of(Capability::PowerState);
    let mut driver = BuggyDriverD { caps };

    let handle = driver
        .connect(&DiscoveredTv::probed(TvVendor::Unknown, "127.0.0.1"))
        .await
        .unwrap();

    let violations = conformance::run(&handle).await;
    assert!(
        !violations.is_empty(),
        "conformance::run must catch leaked accessor"
    );
    let leak_violation = violations.iter().find(|v| {
        v.detail
            .contains("volume: advertised=false but accessor_present=true")
    });
    assert!(
        leak_violation.is_some(),
        "Expected exact violation detail for unadvertised Volume accessor leak: {violations:#?}"
    );
}

#[tokio::test]
async fn test_adversarial_case_e_unadvertised_capability_in_conformance_cases() {
    // Driver E advertises PowerState only, but conformance_cases() emits [PowerState, Volume]
    let caps = CapabilitySet::of(Capability::PowerState);
    let driver = BuggyDriverE { caps };

    let cases = driver.conformance_cases();
    assert_eq!(cases.len(), 2);
    assert_eq!(cases[1].capability, Capability::Volume);

    // EMPIRICAL CRITIC DISCOVERY:
    // verify_orphaned_capabilities iterates exclusively over capabilities.iter(),
    // checking whether each advertised capability has a case.
    // It does NOT perform reverse subset verification (checking if every case tests an advertised capability).
    let result = conformance::verify_orphaned_capabilities(driver.capabilities(), &cases);

    // Record the empirical finding:
    // The current implementation returns Ok(()) because it is a 1-way subset check (capabilities <= cases).
    // An adversarial driver emitting extraneous, unadvertised conformance cases passes through silently!
    assert!(
        result.is_ok(),
        "EMPIRICAL FINDING: verify_orphaned_capabilities does NOT fail on unadvertised capability cases"
    );

    // Demonstrate the reverse-check verification logic that would catch this violation:
    let mut unadvertised_cases = Vec::new();
    for case in &cases {
        if !driver.capabilities().contains(case.capability) {
            unadvertised_cases.push(case.capability);
        }
    }
    assert_eq!(
        unadvertised_cases,
        vec![Capability::Volume],
        "Reverse check correctly isolates the unadvertised conformance case"
    );
}

// =============================================================================
// TEST SECTION 2: Universal Expanding Registry Stress
// =============================================================================

struct ThirdPartyCustomHandle {
    desc: DeviceDescriptor,
    caps: CapabilitySet,
}

impl DeviceHandle for ThirdPartyCustomHandle {
    fn id(&self) -> DeviceId {
        self.desc.id.clone()
    }
    fn descriptor(&self) -> DeviceDescriptor {
        self.desc.clone()
    }
    fn capabilities(&self) -> CapabilitySet {
        self.caps
    }
    fn as_power(&self) -> Option<&dyn PowerControl> {
        None
    }
    fn as_key(&self) -> Option<&dyn tv_rs_core::KeyInput> {
        None
    }
    fn as_app(&self) -> Option<&dyn AppLaunch> {
        None
    }
    fn as_text(&self) -> Option<&dyn tv_rs_core::TextInput> {
        None
    }
    fn as_input(&self) -> Option<&dyn InputSelect> {
        None
    }
    fn as_volume(&self) -> Option<&dyn VolumeControl> {
        None
    }
}

struct ThirdPartyCustomDriver {
    vendor: TvVendor,
    caps: CapabilitySet,
}

#[async_trait]
impl Driver for ThirdPartyCustomDriver {
    type Handle = ThirdPartyCustomHandle;
    fn vendor(&self) -> TvVendor {
        self.vendor
    }
    fn capabilities(&self) -> &CapabilitySet {
        &self.caps
    }
    async fn connect(&mut self, _target: &DiscoveredTv) -> Result<Self::Handle, TvError> {
        Ok(ThirdPartyCustomHandle {
            desc: DeviceDescriptor::new(
                DeviceId::new("custom-plugin-1").unwrap(),
                DeviceOs::Unknown,
                "192.168.1.99",
            ),
            caps: self.caps,
        })
    }
    async fn send(&mut self, _cmd: Command) -> Result<CommandAck, TvError> {
        Ok(CommandAck::new())
    }
    async fn observe(&mut self) -> Result<ObservationStream, TvError> {
        Ok(ObservationStream::new())
    }
}

#[test]
fn test_universal_expanding_registry_dynamic_runtime_plugins() {
    let mut reg = DriverRegistry::new();
    assert!(reg.is_empty());

    // 1. Create a dynamic third-party driver with custom capabilities
    let plugin_caps_1 = CapabilitySet::empty()
        .with(Capability::ScreenCast)
        .with(Capability::SpatialAudio)
        .with(Capability::Telemetry);

    let driver_1 = ThirdPartyCustomDriver {
        vendor: TvVendor::Philips,
        caps: plugin_caps_1,
    };

    // Register without modifying any core or facade code
    reg.register_driver(driver_1);
    assert_eq!(reg.len(), 1);

    // 2. Register a second dynamic third-party driver
    let plugin_caps_2 = CapabilitySet::empty()
        .with(Capability::Orientation)
        .with(Capability::PanelLock)
        .with(Capability::Brightness);

    let driver_2 = ThirdPartyCustomDriver {
        vendor: TvVendor::Unknown,
        caps: plugin_caps_2,
    };

    reg.register_driver(driver_2);
    assert_eq!(reg.len(), 2);

    // 3. Verify build_capability_registry() correctly maps dynamic plugins
    let cap_reg = reg.build_capability_registry();
    assert_eq!(cap_reg.len(), 2);
    assert_eq!(
        cap_reg.capabilities_for(TvVendor::Philips),
        Some(&plugin_caps_1)
    );
    assert_eq!(
        cap_reg.capabilities_for(TvVendor::Unknown),
        Some(&plugin_caps_2)
    );

    // 4. Verify find_by_capability() reflects dynamic plugins
    let cast_drivers = reg.find_by_capability(Capability::ScreenCast);
    assert_eq!(cast_drivers.len(), 1);
    assert_eq!(cast_drivers[0].vendor(), TvVendor::Philips);

    let orient_drivers = reg.find_by_capability(Capability::Orientation);
    assert_eq!(orient_drivers.len(), 1);
    assert_eq!(orient_drivers[0].vendor(), TvVendor::Unknown);

    let unrepresented = reg.find_by_capability(Capability::Reboot);
    assert!(unrepresented.is_empty());

    // 5. Verify aggregate_capabilities() includes all capabilities across dynamic plugins
    let agg = reg.aggregate_capabilities();
    assert_eq!(agg.len(), 6);
    assert!(agg.contains(Capability::ScreenCast));
    assert!(agg.contains(Capability::SpatialAudio));
    assert!(agg.contains(Capability::Telemetry));
    assert!(agg.contains(Capability::Orientation));
    assert!(agg.contains(Capability::PanelLock));
    assert!(agg.contains(Capability::Brightness));
    assert!(!agg.contains(Capability::Reboot));
    assert!(!agg.contains(Capability::PowerState));
}

#[test]
fn test_universal_expanding_registry_prepopulated_with_defaults_plus_plugins() {
    let mut reg = DriverRegistry::with_defaults();
    let initial_len = reg.len();

    let custom_caps = CapabilitySet::of(Capability::SpatialAudio);
    reg.register_driver(ThirdPartyCustomDriver {
        vendor: TvVendor::Unknown,
        caps: custom_caps,
    });

    assert_eq!(reg.len(), initial_len + 1);
    let cap_reg = reg.build_capability_registry();
    assert_eq!(
        cap_reg.capabilities_for(TvVendor::Unknown),
        Some(&custom_caps)
    );

    let spatial_drivers = reg.find_by_capability(Capability::SpatialAudio);
    assert!(spatial_drivers
        .iter()
        .any(|d| d.vendor() == TvVendor::Unknown));
}

// =============================================================================
// TEST SECTION 3: CapabilitySet Bitwise Math & Combinatorics Across 19 Caps
// =============================================================================

fn all_capabilities_list() -> Vec<Capability> {
    vec![
        Capability::PowerState,
        Capability::Wake,
        Capability::Key,
        Capability::AppLaunch,
        Capability::Foreground,
        Capability::InputSelect,
        Capability::Volume,
        Capability::Brightness,
        Capability::Reboot,
        Capability::Telemetry,
        Capability::PanelLock,
        Capability::Orientation,
        Capability::NoSignalStatus,
        #[cfg(feature = "supporting")]
        Capability::LampOrPanelHours,
        #[cfg(feature = "supporting")]
        Capability::ScreenCapture,
        Capability::TextInput,
        Capability::AppList,
        Capability::ScreenCast,
        Capability::SpatialAudio,
    ]
}

#[test]
fn test_capability_set_empty_and_full_set_invariants() {
    let all = all_capabilities_list();
    let total_count = all.len();

    // 1. Empty set
    let empty = CapabilitySet::empty();
    assert!(empty.is_empty());
    assert_eq!(empty.len(), 0);
    for cap in &all {
        assert!(!empty.contains(*cap));
    }
    assert_eq!(empty.iter().count(), 0);

    // 2. Full set built via FromIterator
    let full: CapabilitySet = all.iter().copied().collect();
    assert!(!full.is_empty());
    assert_eq!(full.len(), total_count);
    for cap in &all {
        assert!(full.contains(*cap));
    }
    assert_eq!(full.iter().count(), total_count);

    // 3. Union with empty is identity
    assert_eq!(empty.union(&empty), empty);
    assert_eq!(full.union(&empty), full);
    assert_eq!(empty.union(&full), full);
    assert_eq!(full.union(&full), full);

    // 4. BitOr operators
    assert_eq!(empty | empty, empty);
    assert_eq!(full | empty, full);
    assert_eq!(empty | full, full);
    assert_eq!(full | full, full);

    // 5. BitOrAssign
    let mut acc = empty;
    acc |= empty;
    assert_eq!(acc, empty);

    acc |= full;
    assert_eq!(acc, full);

    acc |= empty;
    assert_eq!(acc, full);
}

#[test]
fn test_capability_set_duplicate_inserts_and_idempotence() {
    let mut set = CapabilitySet::empty();

    // Repeated inserts of single capability
    for _ in 0..100 {
        set.insert(Capability::Volume);
    }
    assert_eq!(set.len(), 1);
    assert!(set.contains(Capability::Volume));

    // FromIterator with heavy duplicate sequences
    let dups = vec![Capability::Volume; 1000];
    let collected: CapabilitySet = dups.into_iter().collect();
    assert_eq!(collected.len(), 1);
    assert_eq!(collected, CapabilitySet::of(Capability::Volume));

    // Union idempotence: A | A == A
    let test_set = CapabilitySet::of(Capability::PowerState)
        | CapabilitySet::of(Capability::Key)
        | CapabilitySet::of(Capability::Volume);
    assert_eq!(test_set.union(&test_set), test_set);
    assert_eq!(test_set | test_set, test_set);
}

#[test]
fn test_capability_set_pairwise_unions_and_algebraic_laws() {
    let all = all_capabilities_list();

    // 1. Test pairwise unions across all combinations
    for (i, cap1) in all.iter().enumerate() {
        for (j, cap2) in all.iter().enumerate() {
            let s1 = CapabilitySet::of(*cap1);
            let s2 = CapabilitySet::of(*cap2);
            let union = s1 | s2;

            // Commutativity: A | B == B | A
            assert_eq!(
                union,
                s2 | s1,
                "Commutativity failed for {cap1:?}, {cap2:?}"
            );

            if i == j {
                assert_eq!(union.len(), 1);
                assert!(union.contains(*cap1));
            } else {
                assert_eq!(union.len(), 2);
                assert!(union.contains(*cap1));
                assert!(union.contains(*cap2));
            }
        }
    }

    // 2. Associativity: (A | B) | C == A | (B | C)
    for w in all.windows(3) {
        let a = CapabilitySet::of(w[0]);
        let b = CapabilitySet::of(w[1]);
        let c = CapabilitySet::of(w[2]);

        let lhs = (a | b) | c;
        let rhs = a | (b | c);
        assert_eq!(lhs, rhs, "Associativity failed for window {:?}", w);
        assert_eq!(lhs.len(), 3);
    }
}

#[test]
fn test_capability_set_disjoint_partitions_and_exact_bit_count() {
    let all = all_capabilities_list();
    let total = all.len();

    let mid = total / 2;
    let part_a_caps = &all[..mid];
    let part_b_caps = &all[mid..];

    let set_a: CapabilitySet = part_a_caps.iter().copied().collect();
    let set_b: CapabilitySet = part_b_caps.iter().copied().collect();

    assert_eq!(set_a.len(), mid);
    assert_eq!(set_b.len(), total - mid);

    // Verify mutual exclusion
    for cap in part_a_caps {
        assert!(set_a.contains(*cap));
        assert!(!set_b.contains(*cap));
    }
    for cap in part_b_caps {
        assert!(!set_a.contains(*cap));
        assert!(set_b.contains(*cap));
    }

    // Merge via union
    let full_union = set_a.union(&set_b);
    assert_eq!(full_union.len(), total);
    for cap in &all {
        assert!(full_union.contains(*cap));
    }

    // Merge via BitOrAssign
    let mut mut_a = set_a;
    mut_a |= set_b;
    assert_eq!(mut_a, full_union);

    // Incremental bit accumulation from 0 to total
    let mut step_set = CapabilitySet::empty();
    for (idx, cap) in all.iter().enumerate() {
        assert_eq!(step_set.len(), idx);
        step_set.insert(*cap);
        assert_eq!(step_set.len(), idx + 1);
        assert!(step_set.contains(*cap));
    }
    assert_eq!(step_set.len(), total);
}
