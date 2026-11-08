//! Adversarial Challenge Test Harness for Universal Expanding Driver Architecture & Dynamic Registry.
//!
//! Verifies:
//! 1. Dynamic third-party plugin registration via `register_driver` and `register`.
//! 2. Target and vendor identification for built-in drivers (Roku, Ir, Mdc, Sicp) and custom plugins.
//! 3. End-to-end connect, handle erasure (`Box<dyn DeviceHandle>`), and command dispatch via `AnyDriver`.
//! 4. Edge cases: empty registry, unknown vendor, duplicate vendor collisions, registration order,
//!    and thread safety (`Send + Sync`) under concurrent multi-threaded access.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

use async_trait::async_trait;
use tv_rs_core::capability::{
    AppLaunch, Capability, CapabilitySet, DeviceHandle, InputSelect, KeyInput, PowerControl,
    TextInput, VolumeControl,
};
use tv_rs_core::command::{Command, CommandAck, CommandId};
use tv_rs_core::descriptor::{DeviceDescriptor, DeviceId, DeviceOs, DiscoveredTv, PowerState};
use tv_rs_core::driver::{BoxedDeviceHandle, Driver, ErasedDriver};
use tv_rs_core::error::{ProtocolError, TvError};
use tv_rs_core::observe::ObservationStream;
use tv_rs_core::types::TvVendor;
use tv_rs_drivers::registry::DriverRegistry;

// -----------------------------------------------------------------------------
// Mock Third-Party Plugin Implementation
// -----------------------------------------------------------------------------

struct ThirdPartyPowerControl {
    state: PowerState,
}

#[async_trait]
impl PowerControl for ThirdPartyPowerControl {
    async fn power_state(&self) -> Result<PowerState, TvError> {
        Ok(self.state)
    }

    async fn wake(&self) -> Result<(), TvError> {
        Ok(())
    }

    async fn sleep(&self) -> Result<(), TvError> {
        Ok(())
    }
}

struct ThirdPartyHandle {
    id: DeviceId,
    descriptor: DeviceDescriptor,
    capabilities: CapabilitySet,
    power: ThirdPartyPowerControl,
}

impl DeviceHandle for ThirdPartyHandle {
    fn id(&self) -> DeviceId {
        self.id.clone()
    }

    fn descriptor(&self) -> DeviceDescriptor {
        self.descriptor.clone()
    }

    fn capabilities(&self) -> CapabilitySet {
        self.capabilities
    }

    fn as_power(&self) -> Option<&dyn PowerControl> {
        Some(&self.power)
    }

    fn as_key(&self) -> Option<&dyn KeyInput> {
        None
    }

    fn as_app(&self) -> Option<&dyn AppLaunch> {
        None
    }

    fn as_text(&self) -> Option<&dyn TextInput> {
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
    capabilities: CapabilitySet,
    connect_calls: Arc<AtomicUsize>,
    send_calls: Arc<AtomicUsize>,
    observe_calls: Arc<AtomicUsize>,
    tag: &'static str,
}

impl ThirdPartyCustomDriver {
    fn new(vendor: TvVendor, tag: &'static str) -> Self {
        Self {
            vendor,
            capabilities: CapabilitySet::empty()
                .with(Capability::PowerState)
                .with(Capability::Wake),
            connect_calls: Arc::new(AtomicUsize::new(0)),
            send_calls: Arc::new(AtomicUsize::new(0)),
            observe_calls: Arc::new(AtomicUsize::new(0)),
            tag,
        }
    }
}

#[async_trait]
impl Driver for ThirdPartyCustomDriver {
    type Handle = ThirdPartyHandle;

    fn vendor(&self) -> TvVendor {
        self.vendor
    }

    fn capabilities(&self) -> &CapabilitySet {
        &self.capabilities
    }

    async fn connect(&mut self, target: &DiscoveredTv) -> Result<Self::Handle, TvError> {
        self.connect_calls.fetch_add(1, Ordering::SeqCst);
        let id_str = format!("plugin-{}-{}-{}", self.tag, self.vendor.as_str(), target.ip);
        let id = DeviceId::new(id_str).unwrap_or_else(|_| DeviceId::anonymous());
        let desc = DeviceDescriptor::new(id.clone(), DeviceOs::Unknown, &target.ip);
        Ok(ThirdPartyHandle {
            id,
            descriptor: desc,
            capabilities: self.capabilities,
            power: ThirdPartyPowerControl {
                state: PowerState::On,
            },
        })
    }

    async fn send(&mut self, cmd: Command) -> Result<CommandAck, TvError> {
        self.send_calls.fetch_add(1, Ordering::SeqCst);
        match cmd {
            Command::Power(_) => Ok(CommandAck::with_id(CommandId::new())),
            Command::Custom { name, .. } if name == "fail_me" => {
                Err(TvError::Protocol(ProtocolError::Malformed("forced test error".to_string())))
            }
            _ => Ok(CommandAck {
                id: None,
                message: Some(format!("Handled by plugin {}", self.tag)),
            }),
        }
    }

    async fn observe(&mut self) -> Result<ObservationStream, TvError> {
        self.observe_calls.fetch_add(1, Ordering::SeqCst);
        Ok(ObservationStream::new())
    }
}

// -----------------------------------------------------------------------------
// Test 1: Dynamic Plugin Registration into Empty & Default Registry
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_dynamic_plugin_registration() {
    // 1. Register into fresh empty registry
    let mut reg = DriverRegistry::new();
    assert!(reg.is_empty());
    assert_eq!(reg.len(), 0);

    let plugin_a = ThirdPartyCustomDriver::new(TvVendor::GoogleCast, "cast_plugin");
    let connect_counter = Arc::clone(&plugin_a.connect_calls);
    let send_counter = Arc::clone(&plugin_a.send_calls);

    reg.register_driver(plugin_a);
    assert_eq!(reg.len(), 1);
    assert!(!reg.is_empty());

    // 2. Query driver by vendor and target
    let driver_ref = reg.find_for_vendor(TvVendor::GoogleCast).expect("found cast plugin");
    assert_eq!(driver_ref.vendor(), TvVendor::GoogleCast);
    assert!(driver_ref.capabilities().contains(Capability::PowerState));

    let target = DiscoveredTv::probed(TvVendor::GoogleCast, "10.0.1.25");
    let driver_for_target = reg.find_for_target(&target).expect("found for target");
    assert_eq!(driver_for_target.vendor(), TvVendor::GoogleCast);

    // 3. Register a second plugin via Box<AnyDriver> (ErasedDriver adapter)
    let plugin_b = ThirdPartyCustomDriver::new(TvVendor::Tizen, "tizen_plugin");
    reg.register(Box::new(ErasedDriver(plugin_b)));
    assert_eq!(reg.len(), 2);
    assert!(reg.find_for_vendor(TvVendor::Tizen).is_some());

    // 4. Test with_defaults() registration expansion
    let mut default_reg = DriverRegistry::with_defaults();
    let initial_count = default_reg.len();
    assert!(initial_count >= 4, "Expected at least roku, mdc, sicp, ir built-ins");

    let plugin_c = ThirdPartyCustomDriver::new(TvVendor::WebOs, "webos_plugin");
    default_reg.register_driver(plugin_c);
    assert_eq!(default_reg.len(), initial_count + 1);
    assert!(default_reg.find_for_vendor(TvVendor::WebOs).is_some());

    // 5. Connect and execute via mutable lookup
    let mut_driver = reg.find_for_vendor_mut(TvVendor::GoogleCast).expect("mutable driver");
    let handle = mut_driver.connect(&target).await.expect("connect succeeds");
    assert_eq!(connect_counter.load(Ordering::SeqCst), 1);
    assert_eq!(handle.id().as_str(), "plugin-cast_plugin-googlecast-10.0.1.25");

    let ack = mut_driver.send(Command::Power(true)).await.expect("send succeeds");
    assert_eq!(send_counter.load(Ordering::SeqCst), 1);
    assert!(ack.id.is_some());
}

// -----------------------------------------------------------------------------
// Test 2: Target & Vendor Identification for Built-in Drivers
// -----------------------------------------------------------------------------

#[test]
fn test_builtin_driver_resolution() {
    let reg = DriverRegistry::with_defaults();

    // Verify built-in vendors in Wave 1
    let roku_driver = reg.find_for_vendor(TvVendor::Roku).expect("Roku driver must be registered");
    assert_eq!(roku_driver.vendor(), TvVendor::Roku);

    let mdc_driver = reg.find_for_vendor(TvVendor::Mdc).expect("MDC driver must be registered");
    assert_eq!(mdc_driver.vendor(), TvVendor::Mdc);

    let sicp_driver = reg.find_for_vendor(TvVendor::Sicp).expect("SICP driver must be registered");
    assert_eq!(sicp_driver.vendor(), TvVendor::Sicp);

    let ir_driver = reg.find_for_vendor(TvVendor::Ir).expect("IR driver must be registered");
    assert_eq!(ir_driver.vendor(), TvVendor::Ir);

    // Verify find_for_target
    let target_roku = DiscoveredTv::probed(TvVendor::Roku, "192.168.1.100");
    assert_eq!(reg.find_for_target(&target_roku).map(|d| d.vendor()), Some(TvVendor::Roku));

    let target_mdc = DiscoveredTv::probed(TvVendor::Mdc, "192.168.1.101");
    assert_eq!(reg.find_for_target(&target_mdc).map(|d| d.vendor()), Some(TvVendor::Mdc));

    let target_sicp = DiscoveredTv::probed(TvVendor::Sicp, "192.168.1.102");
    assert_eq!(reg.find_for_target(&target_sicp).map(|d| d.vendor()), Some(TvVendor::Sicp));

    let target_ir = DiscoveredTv::probed(TvVendor::Ir, "192.168.1.103");
    assert_eq!(reg.find_for_target(&target_ir).map(|d| d.vendor()), Some(TvVendor::Ir));

    // Target with unmapped vendor returns None
    let target_unknown = DiscoveredTv::probed(TvVendor::Unknown, "192.168.1.104");
    assert!(reg.find_for_target(&target_unknown).is_none());
}

// -----------------------------------------------------------------------------
// Test 3: Connect, Command Dispatch & State Observation via AnyDriver
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_any_driver_polymorphic_dispatch() {
    let mut reg = DriverRegistry::new();
    let plugin = ThirdPartyCustomDriver::new(TvVendor::Samsung, "custom_samsung");
    reg.register_driver(plugin);

    let driver = reg.find_for_vendor_mut(TvVendor::Samsung).expect("driver found");

    // 1. Connect returning erased handle
    let target = DiscoveredTv::probed(TvVendor::Samsung, "172.16.0.5");
    let handle: BoxedDeviceHandle = driver.connect(&target).await.expect("connect ok");

    assert_eq!(handle.descriptor().address, "172.16.0.5");
    assert!(handle.capabilities().contains(Capability::PowerState));

    // Test downcast/accessor on erased handle
    let power_opt = handle.as_power();
    assert!(power_opt.is_some());
    let power = power_opt.unwrap();
    let state = power.power_state().await.expect("power state ok");
    assert_eq!(state, PowerState::On);

    // 2. Command dispatch
    let ack = driver.send(Command::Power(false)).await.expect("send ok");
    assert!(ack.id.is_some());

    // 3. Command error propagation
    let err_ack = driver
        .send(Command::Custom {
            name: "fail_me".to_string(),
            payload: "none".to_string(),
        })
        .await;
    match err_ack {
        Err(TvError::Protocol(ProtocolError::Malformed(msg))) => {
            assert_eq!(msg, "forced test error");
        }
        other => panic!("expected ProtocolError::Malformed, got {other:?}"),
    }

    // 4. Observation stream dispatch
    let stream_res = driver.observe().await;
    assert!(stream_res.is_ok());
}

// -----------------------------------------------------------------------------
// Test 4: Edge Cases — Registration Order, Duplicates, and Lookup Semantics
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_edge_cases_duplicates_and_order() {
    let mut reg = DriverRegistry::new();

    // Register two drivers for the SAME vendor (TvVendor::Roku) with different tags/capabilities
    let mut driver_v1 = ThirdPartyCustomDriver::new(TvVendor::Roku, "v1");
    driver_v1.capabilities = CapabilitySet::empty().with(Capability::Wake);

    let mut driver_v2 = ThirdPartyCustomDriver::new(TvVendor::Roku, "v2");
    driver_v2.capabilities = CapabilitySet::empty().with(Capability::Volume);

    reg.register_driver(driver_v1);
    reg.register_driver(driver_v2);

    assert_eq!(reg.len(), 2);

    // Dynamic registry semantics: find_for_vendor returns the FIRST registered driver (FIFO)
    let found = reg.find_for_vendor(TvVendor::Roku).expect("driver found");
    assert!(
        found.capabilities().contains(Capability::Wake),
        "Expected first registered driver (v1) to be returned"
    );
    assert!(!found.capabilities().contains(Capability::Volume));

    // All drivers are preserved in registry.drivers()
    let drivers = reg.drivers();
    assert_eq!(drivers.len(), 2);
    assert!(drivers[0].capabilities().contains(Capability::Wake));
    assert!(drivers[1].capabilities().contains(Capability::Volume));

    // Consuming registry into Vec
    let into_vec = reg.into_drivers();
    assert_eq!(into_vec.len(), 2);
}

#[test]
fn test_edge_cases_empty_and_unknown() {
    let reg = DriverRegistry::new();
    assert_eq!(reg.len(), 0);
    assert!(reg.is_empty());
    assert!(reg.drivers().is_empty());

    // Non-existent vendor lookups return None safely without panicking
    assert!(reg.find_for_vendor(TvVendor::Unknown).is_none());
    assert!(reg.find_for_vendor(TvVendor::Roku).is_none());
    assert!(reg.find_for_target(&DiscoveredTv::probed(TvVendor::Ir, "1.2.3.4")).is_none());
}

// -----------------------------------------------------------------------------
// Test 5: Concurrency and Thread Safety (Send + Sync) Stress Test
// -----------------------------------------------------------------------------

#[test]
fn test_thread_safety_and_concurrent_queries() {
    // Wrap registry in Arc to stress concurrent reads across multiple OS threads
    let default_reg = Arc::new(DriverRegistry::with_defaults());

    let mut handles = Vec::new();
    for i in 0..16 {
        let reg_clone = Arc::clone(&default_reg);
        let handle = thread::spawn(move || {
            for round in 0..100 {
                let vendor = match (i + round) % 4 {
                    0 => TvVendor::Roku,
                    1 => TvVendor::Mdc,
                    2 => TvVendor::Sicp,
                    _ => TvVendor::Ir,
                };
                let driver = reg_clone.find_for_vendor(vendor).expect("concurrent find succeeds");
                assert_eq!(driver.vendor(), vendor);

                let target = DiscoveredTv::probed(vendor, format!("192.168.1.{}", (i * 10 + round) % 250));
                let target_driver = reg_clone.find_for_target(&target).expect("concurrent target find succeeds");
                assert_eq!(target_driver.vendor(), vendor);
            }
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().expect("thread completed successfully");
    }
}

// -----------------------------------------------------------------------------
// Test 6: Built-in Driver Connect, Send & Error Routing via AnyDriver
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_builtin_ir_driver_dispatch_and_unconnected_rejections() {
    let mut reg = DriverRegistry::with_defaults();

    // 1. IrDriver end-to-end connect and command dispatch via dynamic AnyDriver
    let ir = reg.find_for_vendor_mut(TvVendor::Ir).expect("IR driver present");
    let ir_target = DiscoveredTv::probed(TvVendor::Ir, "Sony-LivingRoom");
    let handle = ir.connect(&ir_target).await.expect("IR connect succeeds");
    assert!(handle.capabilities().contains(Capability::Key));

    let ack = ir.send(Command::Power(true)).await.expect("IR power send succeeds");
    assert!(ack.id.is_some());

    let err_ack = ir.send(Command::Custom {
        name: "unknown_ir_cmd".to_string(),
        payload: "".to_string(),
    }).await;
    assert!(matches!(err_ack, Err(TvError::Protocol(ProtocolError::Malformed(_)))));

    // 2. RokuDriver unconnected send rejection
    let roku = reg.find_for_vendor_mut(TvVendor::Roku).expect("Roku driver present");
    let roku_err = roku.send(Command::Power(true)).await;
    assert!(
        matches!(roku_err, Err(TvError::Transport(_))),
        "Expected Transport error when sending to unconnected Roku driver, got {roku_err:?}"
    );

    // 3. MdcDriver unconnected send rejection
    let mdc = reg.find_for_vendor_mut(TvVendor::Mdc).expect("MDC driver present");
    let mdc_err = mdc.send(Command::Power(true)).await;
    assert!(
        matches!(mdc_err, Err(TvError::Transport(_))),
        "Expected Transport error when sending to unconnected MDC driver, got {mdc_err:?}"
    );

    // 4. MdcDriver empty target rejection
    let empty_target = DiscoveredTv::probed(TvVendor::Mdc, "   ");
    let mdc_conn_err = mdc.connect(&empty_target).await;
    match mdc_conn_err {
        Err(TvError::Protocol(_)) => {}
        Err(e) => panic!("Expected Protocol error for empty target, got {e}"),
        Ok(_) => panic!("Expected error for empty target, got Ok"),
    }
}

// -----------------------------------------------------------------------------
// Test 7: Concurrent Read/Write under tokio::sync::RwLock
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_concurrent_read_write_rwlock() {
    use tokio::sync::RwLock;

    let shared_reg = Arc::new(RwLock::new(DriverRegistry::with_defaults()));
    let mut tasks = Vec::new();

    // Spawn 8 reader tasks
    for i in 0..8 {
        let reg = Arc::clone(&shared_reg);
        tasks.push(tokio::spawn(async move {
            for _ in 0..50 {
                let r = reg.read().await;
                assert!(r.len() >= 4);
                assert!(r.find_for_vendor(TvVendor::Roku).is_some());
                assert!(r.find_for_vendor(TvVendor::Mdc).is_some());
                tokio::task::yield_now().await;
            }
            i
        }));
    }

    // Spawn 2 writer tasks dynamically expanding the registry
    for w in 0..2 {
        let reg = Arc::clone(&shared_reg);
        tasks.push(tokio::spawn(async move {
            for step in 0..10 {
                let vendor = if (w + step) % 2 == 0 {
                    TvVendor::GoogleCast
                } else {
                    TvVendor::Tizen
                };
                let plugin = ThirdPartyCustomDriver::new(vendor, "dynamic_writer");
                {
                    let mut write_guard = reg.write().await;
                    write_guard.register_driver(plugin);
                }
                tokio::task::yield_now().await;
            }
            100 + w
        }));
    }

    for task in tasks {
        task.await.expect("task finished cleanly");
    }

    let final_reg = shared_reg.read().await;
    assert!(final_reg.len() >= 4 + 20, "Registry should contain all dynamically registered drivers");
}
