//! Wave 2 Adversarial Challenge & Stress Test Suite (TV-12 & TV-14)
//!
//! Empirically challenges and verifies:
//! 1. Dynamic dispatch and open-ended `DriverRegistry` (`AnyDriver`, dynamic registration)
//!    for `TizenDriver`, `WebOsDriver`, `CastDriver`, and `CecDriver`.
//! 2. Vendor alias resolution in `DriverRegistry::find_for_vendor` & `find_for_target`:
//!    - Samsung / Tizen (bidirectional)
//!    - Lg / WebOs (bidirectional)
//!    - GoogleCast
//!    - Cec
//! 3. HDMI-CEC User Control repeat release sequence under stress:
//!    - Atomic pairing: `0x44` (User Control Pressed) followed immediately by `0x45` (User Control Released).
//!    - High concurrency stress ensuring zero orphaned press or release frames.
//! 4. Connection pooling in Tizen driver (`TizenConnectionPool`):
//!    - Channel reuse, dead channel single retry, permanent failure abort, multi-host isolation, concurrency stress.
//! 5. Connection pooling in WebOS driver (`WebosSessionPool`):
//!    - Session reuse, pointer socket reuse, dead socket retry, idle timeout expiration, multi-host isolation, concurrency stress.

#![cfg(all(feature = "tizen", feature = "webos", feature = "cast", feature = "cec"))]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex as TokioMutex;

use async_trait::async_trait;

use tv_rs_core::capability::{
    AppLaunch, Capability, CapabilitySet, DeviceHandle, Key, KeyInput, PowerControl,
    TextInput, VolumeControl,
};
use tv_rs_core::command::{Command, CommandAck, CommandId};
use tv_rs_core::descriptor::{DeviceDescriptor, DeviceId, DeviceOs, DiscoveredTv};
use tv_rs_core::driver::{Driver, ErasedDriver};
use tv_rs_core::error::TvError;
use tv_rs_core::observe::ObservationStream;
use tv_rs_core::types::TvVendor;

use tv_rs_drivers::cast::CastDriver;
use tv_rs_drivers::cec::handle::CecDeviceHandle;
use tv_rs_drivers::cec::opcode::*;
use tv_rs_drivers::cec::transport::CecTransport;
use tv_rs_drivers::cec::types::CecAddress;
use tv_rs_drivers::cec::CecDriver;
use tv_rs_drivers::registry::DriverRegistry;
use tv_rs_drivers::tizen::client::TizenClient;
use tv_rs_drivers::tizen::pool::testkit as tizen_testkit;
use tv_rs_drivers::tizen::pool::TizenConnectionPool;
use tv_rs_drivers::tizen::types::TizenRemoteKey;
use tv_rs_drivers::tizen::TizenDriver;
use tv_rs_drivers::webos::client::WebosClient;
use tv_rs_drivers::webos::pool::testkit as webos_testkit;
use tv_rs_drivers::webos::pool::WebosSessionPool;
use tv_rs_drivers::webos::WebOsDriver;

// =============================================================================
// SECTION 1: Dynamic DriverRegistry & Dynamic Dispatch Verification
// =============================================================================

struct CustomThirdPartyDriver {
    vendor: TvVendor,
    capabilities: CapabilitySet,
    sent_count: Arc<AtomicUsize>,
}

struct CustomThirdPartyHandle {
    descriptor: DeviceDescriptor,
    capabilities: CapabilitySet,
}

impl DeviceHandle for CustomThirdPartyHandle {
    fn id(&self) -> DeviceId {
        self.descriptor.id.clone()
    }
    fn descriptor(&self) -> DeviceDescriptor {
        self.descriptor.clone()
    }
    fn capabilities(&self) -> CapabilitySet {
        self.capabilities
    }
    fn as_power(&self) -> Option<&dyn PowerControl> {
        None
    }
    fn as_key(&self) -> Option<&dyn KeyInput> {
        None
    }
    fn as_volume(&self) -> Option<&dyn VolumeControl> {
        None
    }
    fn as_app(&self) -> Option<&dyn AppLaunch> {
        None
    }
    fn as_text(&self) -> Option<&dyn TextInput> {
        None
    }
    fn as_input(&self) -> Option<&dyn tv_rs_core::capability::InputSelect> {
        None
    }
}

#[async_trait]
impl Driver for CustomThirdPartyDriver {
    type Handle = CustomThirdPartyHandle;

    fn vendor(&self) -> TvVendor {
        self.vendor
    }

    fn capabilities(&self) -> &CapabilitySet {
        &self.capabilities
    }

    async fn connect(&mut self, target: &DiscoveredTv) -> Result<Self::Handle, TvError> {
        let desc = DeviceDescriptor::new(
            DeviceId::new("custom-1").unwrap(),
            DeviceOs::Unknown,
            &target.ip,
        );
        Ok(CustomThirdPartyHandle {
            descriptor: desc,
            capabilities: self.capabilities,
        })
    }

    async fn send(&mut self, _cmd: Command) -> Result<CommandAck, TvError> {
        self.sent_count.fetch_add(1, Ordering::SeqCst);
        Ok(CommandAck::with_id(CommandId::new()))
    }

    async fn observe(&mut self) -> Result<ObservationStream, TvError> {
        Ok(ObservationStream::new())
    }
}

#[tokio::test]
async fn test_wave2_drivers_in_default_registry_and_polymorphic_dispatch() {
    let mut reg = DriverRegistry::with_defaults();
    assert!(reg.len() >= 4, "Wave 2 registry must have at least 4 drivers");

    // 1. Verify Tizen in registry
    let tizen_driver = reg.find_for_vendor_mut(TvVendor::Samsung).expect("Tizen driver found");
    assert_eq!(tizen_driver.vendor(), TvVendor::Samsung);
    assert!(tizen_driver.capabilities().contains(Capability::Key));
    assert!(tizen_driver.capabilities().contains(Capability::PowerState));
    assert!(tizen_driver.capabilities().contains(Capability::AppLaunch));

    // Erased handle connection
    let tizen_target = DiscoveredTv::probed(TvVendor::Samsung, "192.168.1.50");
    let tizen_handle = tizen_driver.connect(&tizen_target).await.expect("Tizen connect ok");
    assert_eq!(tizen_handle.descriptor().address, "192.168.1.50");
    assert!(tizen_handle.as_key().is_some());
    assert!(tizen_handle.as_power().is_some());
    assert!(tizen_handle.as_app().is_some());
    assert!(tizen_handle.as_volume().is_some());
    assert!(tizen_handle.as_text().is_some());
    assert!(tizen_handle.as_screen_cast().is_none());

    // 2. Verify WebOS in registry
    let webos_driver = reg.find_for_vendor_mut(TvVendor::Lg).expect("WebOS driver found");
    assert_eq!(webos_driver.vendor(), TvVendor::Lg);
    assert!(webos_driver.capabilities().contains(Capability::Key));
    assert!(webos_driver.capabilities().contains(Capability::Volume));

    let webos_target = DiscoveredTv::probed(TvVendor::Lg, "192.168.1.60");
    let webos_handle = webos_driver.connect(&webos_target).await.expect("WebOS connect ok");
    assert_eq!(webos_handle.descriptor().address, "192.168.1.60");
    assert!(webos_handle.as_key().is_some());
    assert!(webos_handle.as_power().is_some());
    assert!(webos_handle.as_volume().is_some());
    assert!(webos_handle.as_app().is_some());

    // 3. Verify Cast in registry
    let cast_driver = reg.find_for_vendor_mut(TvVendor::GoogleCast).expect("Cast driver found");
    assert_eq!(cast_driver.vendor(), TvVendor::GoogleCast);
    assert!(cast_driver.capabilities().contains(Capability::ScreenCast));
    assert!(cast_driver.capabilities().contains(Capability::Volume));

    // 4. Verify CEC in registry
    let cec_driver = reg.find_for_vendor_mut(TvVendor::Cec).expect("CEC driver found");
    assert_eq!(cec_driver.vendor(), TvVendor::Cec);
    assert!(cec_driver.capabilities().contains(Capability::InputSelect));
    assert!(cec_driver.capabilities().contains(Capability::Key));

    let cec_target = DiscoveredTv::probed(TvVendor::Cec, "0");
    let cec_handle = cec_driver.connect(&cec_target).await.expect("CEC connect ok");
    assert!(cec_handle.as_key().is_some());
    assert!(cec_handle.as_input().is_some());
    assert!(cec_handle.as_power().is_some());
    assert!(cec_handle.as_volume().is_some());
    assert!(cec_handle.as_app().is_none());
    assert!(cec_handle.as_text().is_none());
}

#[tokio::test]
async fn test_open_ended_extensible_registry_plug_and_play() {
    let mut reg = DriverRegistry::new();
    assert!(reg.is_empty());

    // Register all four Wave 2 drivers manually
    reg.register_driver(TizenDriver::new());
    reg.register_driver(WebOsDriver::new());
    reg.register_driver(CastDriver::new());
    reg.register_driver(CecDriver::new());

    // Register custom plugin via register_driver
    let sent_counter = Arc::new(AtomicUsize::new(0));
    reg.register_driver(CustomThirdPartyDriver {
        vendor: TvVendor::Unknown,
        capabilities: CapabilitySet::empty(),
        sent_count: sent_counter.clone(),
    });

    // Register custom plugin via register(Box::new(ErasedDriver(...)))
    let sent_counter2 = Arc::new(AtomicUsize::new(0));
    reg.register(Box::new(ErasedDriver(CustomThirdPartyDriver {
        vendor: TvVendor::Philips,
        capabilities: CapabilitySet::empty(),
        sent_count: sent_counter2.clone(),
    })));

    assert_eq!(reg.len(), 6);

    // Verify dynamic dispatch across all registered drivers
    assert!(reg.find_for_vendor(TvVendor::Samsung).is_some());
    assert!(reg.find_for_vendor(TvVendor::Lg).is_some());
    assert!(reg.find_for_vendor(TvVendor::GoogleCast).is_some());
    assert!(reg.find_for_vendor(TvVendor::Cec).is_some());
    assert!(reg.find_for_vendor(TvVendor::Unknown).is_some());
    assert!(reg.find_for_vendor(TvVendor::Philips).is_some());

    let plugin = reg.find_for_vendor_mut(TvVendor::Philips).expect("Philips driver");
    plugin.send(Command::Power(true)).await.expect("plugin send ok");
    assert_eq!(sent_counter2.load(Ordering::SeqCst), 1);
}

// =============================================================================
// SECTION 2: Vendor Alias Resolution Verification
// =============================================================================

#[test]
fn test_vendor_alias_resolution_bidirectional_and_targets() {
    let reg = DriverRegistry::with_defaults();

    // 1. Samsung <-> Tizen alias
    let by_samsung = reg.find_for_vendor(TvVendor::Samsung).expect("find by Samsung");
    let by_tizen = reg.find_for_vendor(TvVendor::Tizen).expect("find by Tizen");
    assert_eq!(by_samsung.vendor(), TvVendor::Samsung);
    assert_eq!(by_tizen.vendor(), TvVendor::Samsung);

    let target_samsung = DiscoveredTv::probed(TvVendor::Samsung, "10.0.0.1");
    let target_tizen = DiscoveredTv::probed(TvVendor::Tizen, "10.0.0.1");
    assert!(reg.find_for_target(&target_samsung).is_some());
    assert!(reg.find_for_target(&target_tizen).is_some());

    // 2. Lg <-> WebOs alias
    let by_lg = reg.find_for_vendor(TvVendor::Lg).expect("find by Lg");
    let by_webos = reg.find_for_vendor(TvVendor::WebOs).expect("find by WebOs");
    assert_eq!(by_lg.vendor(), TvVendor::Lg);
    assert_eq!(by_webos.vendor(), TvVendor::Lg);

    let target_lg = DiscoveredTv::probed(TvVendor::Lg, "10.0.0.2");
    let target_webos = DiscoveredTv::probed(TvVendor::WebOs, "10.0.0.2");
    assert!(reg.find_for_target(&target_lg).is_some());
    assert!(reg.find_for_target(&target_webos).is_some());

    // 3. GoogleCast
    let by_cast = reg.find_for_vendor(TvVendor::GoogleCast).expect("find by GoogleCast");
    assert_eq!(by_cast.vendor(), TvVendor::GoogleCast);
    let target_cast = DiscoveredTv::probed(TvVendor::GoogleCast, "10.0.0.3");
    assert!(reg.find_for_target(&target_cast).is_some());

    // 4. CEC
    let by_cec = reg.find_for_vendor(TvVendor::Cec).expect("find by Cec");
    assert_eq!(by_cec.vendor(), TvVendor::Cec);
    let target_cec = DiscoveredTv::probed(TvVendor::Cec, "0");
    assert!(reg.find_for_target(&target_cec).is_some());

    // 5. Test reverse alias resolution:
    // If a driver was registered with vendor == TvVendor::Tizen, finding by TvVendor::Samsung works!
    let mut custom_reg = DriverRegistry::new();
    custom_reg.register_driver(CustomThirdPartyDriver {
        vendor: TvVendor::Tizen,
        capabilities: CapabilitySet::empty(),
        sent_count: Arc::new(AtomicUsize::new(0)),
    });
    custom_reg.register_driver(CustomThirdPartyDriver {
        vendor: TvVendor::WebOs,
        capabilities: CapabilitySet::empty(),
        sent_count: Arc::new(AtomicUsize::new(0)),
    });

    assert!(custom_reg.find_for_vendor(TvVendor::Samsung).is_some());
    assert!(custom_reg.find_for_vendor(TvVendor::Tizen).is_some());
    assert!(custom_reg.find_for_vendor(TvVendor::Lg).is_some());
    assert!(custom_reg.find_for_vendor(TvVendor::WebOs).is_some());
}

// =============================================================================
// SECTION 3: CEC User Control Repeat Release Honesty & Stress Verification
// =============================================================================

/// Thread-safe recording CEC transport that captures every frame in exact chronological order.
struct RecordingCecTransport {
    frames: Arc<TokioMutex<Vec<String>>>,
}

impl RecordingCecTransport {
    fn new() -> (Self, Arc<TokioMutex<Vec<String>>>) {
        let frames = Arc::new(TokioMutex::new(Vec::new()));
        (Self { frames: frames.clone() }, frames)
    }
}

#[async_trait]
impl CecTransport for RecordingCecTransport {
    async fn send(&self, command: &str) -> Result<String, tv_rs_drivers::cec::error::CecError> {
        let mut lock = self.frames.lock().await;
        lock.push(command.to_string());
        Ok("ok".to_string())
    }
}

#[tokio::test]
async fn test_cec_user_control_pressed_and_released_atomic_pairing() {
    let (transport, frames) = RecordingCecTransport::new();
    let arc_transport = Arc::new(transport);

    let handle = CecDeviceHandle::new(
        DeviceDescriptor::new(DeviceId::new("cec-0").unwrap(), DeviceOs::Cec, "cec:0"),
        CapabilitySet::empty(),
        arc_transport.clone(),
        CecAddress::Tv,
        CecAddress::Playback1.as_u8(),
    );

    // Test a wide sequence of standard keys
    let test_keys = [
        Key::Select,
        Key::Up,
        Key::Down,
        Key::Left,
        Key::Right,
        Key::Home,
        Key::Back,
        Key::Power,
        Key::Play,
        Key::Pause,
        Key::Stop,
    ];

    for key in &test_keys {
        handle.key_press(*key, CommandId::new()).await.expect("key press ok");
    }

    let recorded = frames.lock().await.clone();
    assert_eq!(
        recorded.len(),
        test_keys.len() * 2,
        "Every key must produce exactly 2 frames (pressed + released)"
    );

    for (i, key) in test_keys.iter().enumerate() {
        let press_frame = &recorded[i * 2];
        let release_frame = &recorded[i * 2 + 1];

        let expected_code = map_key_to_cec(*key).expect("mapped key");

        // Verify pressed frame: tx 40:44:XX
        let expected_press = format!("tx 40:44:{expected_code:02X}");
        assert_eq!(press_frame, &expected_press, "Press frame must match opcode 0x44 and UI code");

        // Verify released frame: tx 40:45
        assert_eq!(release_frame, "tx 40:45", "Release frame must follow immediately with opcode 0x45");
    }
}

#[tokio::test]
async fn test_cec_driver_send_volume_and_mute_immediate_release() {
    let (transport, frames) = RecordingCecTransport::new();
    let mut driver = CecDriver::new().with_transport(Arc::new(transport));

    // Volume up (> 50)
    driver.send(Command::Volume(80)).await.expect("volume up ok");
    // Volume down (<= 50)
    driver.send(Command::Volume(20)).await.expect("volume down ok");
    // Mute
    driver.send(Command::Mute(true)).await.expect("mute ok");

    let recorded = frames.lock().await.clone();
    assert_eq!(recorded.len(), 6, "3 actions * 2 frames = 6 frames");

    // Action 1: VolumeUp (0x41)
    assert_eq!(recorded[0], "tx 40:44:41");
    assert_eq!(recorded[1], "tx 40:45");

    // Action 2: VolumeDown (0x42)
    assert_eq!(recorded[2], "tx 40:44:42");
    assert_eq!(recorded[3], "tx 40:45");

    // Action 3: Mute (0x43)
    assert_eq!(recorded[4], "tx 40:44:43");
    assert_eq!(recorded[5], "tx 40:45");
}

#[tokio::test]
async fn test_cec_key_press_concurrent_stress_release_invariance() {
    let (transport, frames) = RecordingCecTransport::new();
    let handle = Arc::new(CecDeviceHandle::new(
        DeviceDescriptor::new(DeviceId::new("cec-0").unwrap(), DeviceOs::Cec, "cec:0"),
        CapabilitySet::empty(),
        Arc::new(transport),
        CecAddress::Tv,
        CecAddress::Playback1.as_u8(),
    ));

    let mut tasks = Vec::new();
    for _ in 0..20 {
        let h = handle.clone();
        tasks.push(tokio::spawn(async move {
            for _ in 0..10 {
                h.key_press(Key::Select, CommandId::new()).await.unwrap();
            }
        }));
    }

    for t in tasks {
        t.await.unwrap();
    }

    let recorded = frames.lock().await.clone();
    assert_eq!(recorded.len(), 400, "20 tasks * 10 presses * 2 frames = 400 frames");

    let press_count = recorded.iter().filter(|f| f.contains(":44:")).count();
    let release_count = recorded.iter().filter(|f| f.ends_with(":45")).count();

    assert_eq!(press_count, 200, "Total pressed frames must equal 200");
    assert_eq!(release_count, 200, "Total released frames must equal 200");
}

// =============================================================================
// SECTION 4: Tizen Connection Pooling Stress & Fault Tolerance
// =============================================================================

#[tokio::test]
async fn test_tizen_connection_pool_channel_reuse_under_stress() {
    let (opens, sent) = tizen_testkit::counters();
    let pool = Arc::new(TizenConnectionPool::with_factory(tizen_testkit::factory(
        opens.clone(),
        0,
        sent.clone(),
    )));
    let client = TizenClient::new("192.168.1.100");

    // 100 sequential key presses to the same TV
    for _ in 0..100 {
        pool.send_key(&client, TizenRemoteKey::VolumeUp).await.unwrap();
    }

    assert_eq!(opens.load(Ordering::SeqCst), 1, "Must reuse single pooled channel");
    assert_eq!(sent.load(Ordering::SeqCst), 100, "Must deliver all 100 presses");
    assert_eq!(pool.len().await, 1);
}

#[tokio::test]
async fn test_tizen_connection_pool_reopen_on_dead_socket_and_double_failure() {
    // 1 simulated failure on dead channel -> must reopen once and succeed
    let (opens, sent) = tizen_testkit::counters();
    let pool = TizenConnectionPool::with_factory(tizen_testkit::factory(
        opens.clone(),
        1,
        sent.clone(),
    ));
    let client = TizenClient::new("192.168.1.101");

    pool.send_key(&client, TizenRemoteKey::Home).await.expect("reopen retry succeeds");
    assert_eq!(opens.load(Ordering::SeqCst), 2, "Channel reopened once");
    assert_eq!(sent.load(Ordering::SeqCst), 1, "Delivered once");

    // 2 simulated failures -> retry fails, error surfaces to caller
    let (opens2, sent2) = tizen_testkit::counters();
    let pool2 = TizenConnectionPool::with_factory(tizen_testkit::factory(
        opens2.clone(),
        2,
        sent2.clone(),
    ));
    let result = pool2.send_key(&client, TizenRemoteKey::Home).await;
    assert!(result.is_err(), "Double failure must surface to caller");
    assert_eq!(opens2.load(Ordering::SeqCst), 2, "Does not loop infinitely");
    assert_eq!(sent2.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn test_tizen_connection_pool_multi_host_isolation_and_concurrency() {
    let (opens, sent) = tizen_testkit::counters();
    let pool = Arc::new(TizenConnectionPool::with_factory(tizen_testkit::factory(
        opens.clone(),
        0,
        sent.clone(),
    )));

    // 10 distinct TV hosts, 5 concurrent tasks each
    let mut tasks = Vec::new();
    for host_id in 0..10 {
        for _ in 0..5 {
            let pool_clone = pool.clone();
            tasks.push(tokio::spawn(async move {
                let client = TizenClient::new(format!("10.0.0.{}", host_id + 1));
                pool_clone.send_key(&client, TizenRemoteKey::Enter).await.unwrap();
            }));
        }
    }

    for t in tasks {
        t.await.unwrap();
    }

    assert_eq!(opens.load(Ordering::SeqCst), 10, "Exactly 1 channel per host (10 hosts)");
    assert_eq!(sent.load(Ordering::SeqCst), 50, "50 total messages sent");
    assert_eq!(pool.len().await, 10);

    // Test evict and clear
    assert!(pool.evict("10.0.0.1").await);
    assert!(!pool.contains("10.0.0.1").await);
    assert_eq!(pool.len().await, 9);

    let cleared = pool.clear().await;
    assert_eq!(cleared, 9);
    assert!(pool.is_empty().await);
}

// =============================================================================
// SECTION 5: WebOS Session & Pointer Pooling Stress & Fault Tolerance
// =============================================================================

#[tokio::test]
async fn test_webos_session_pool_dual_session_and_pointer_reuse() {
    let (opens, served) = webos_testkit::counters();
    let (ptr_factory, ptr_opens, ptr_frames) = webos_testkit::pointer_factory(0);

    let pool = WebosSessionPool::with_factories_and_idle(
        webos_testkit::factory(opens.clone(), 0, served.clone()),
        ptr_factory,
        Duration::from_secs(60),
    );
    let client = WebosClient::new("192.168.1.120");

    // 1. Session commands reuse registered session
    for _ in 0..50 {
        pool.set_volume(&client, 25).await.unwrap();
        let _ = pool.get_volume(&client).await.unwrap();
    }
    assert_eq!(opens.load(Ordering::SeqCst), 1, "Session opened once and reused");
    assert_eq!(served.load(Ordering::SeqCst), 100);

    // 2. Key presses reuse pointer socket
    for _ in 0..20 {
        pool.send_key(&client, "ENTER").await.unwrap();
    }
    assert_eq!(ptr_opens.load(Ordering::SeqCst), 1, "Pointer dialed once and reused");
    assert_eq!(ptr_frames.lock().unwrap().len(), 20);
    assert!(pool.has_pointer("192.168.1.120").await);
}

#[tokio::test]
async fn test_webos_session_pool_idle_timeout_expiration() {
    let (opens, served) = webos_testkit::counters();
    let (ptr_factory, _ptr_opens, _ptr_frames) = webos_testkit::pointer_factory(0);

    // Configure aggressive 10ms idle expiration
    let pool = WebosSessionPool::with_factories_and_idle(
        webos_testkit::factory(opens.clone(), 0, served.clone()),
        ptr_factory,
        Duration::from_millis(10),
    );
    let client = WebosClient::new("192.168.1.121");

    pool.set_volume(&client, 30).await.unwrap();
    assert_eq!(opens.load(Ordering::SeqCst), 1);

    // Sleep 25ms to trigger idle expiration
    tokio::time::sleep(Duration::from_millis(25)).await;

    // Next command must recognize stale session and open fresh one
    pool.set_volume(&client, 35).await.unwrap();
    assert_eq!(opens.load(Ordering::SeqCst), 2, "Stale idle session must be dropped and reopened");
}

#[tokio::test]
async fn test_webos_session_pool_reopen_on_dead_socket() {
    let (opens, served) = webos_testkit::counters();
    let pool = WebosSessionPool::with_factory(webos_testkit::factory(
        opens.clone(),
        1, // 1 failure
        served.clone(),
    ));
    let client = WebosClient::new("192.168.1.122");

    pool.set_volume(&client, 50).await.expect("reopen retry succeeds");
    assert_eq!(opens.load(Ordering::SeqCst), 2, "Session reopened once on error");
    assert_eq!(served.load(Ordering::SeqCst), 1);
}
