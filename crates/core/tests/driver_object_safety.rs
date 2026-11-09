//! Empirical verification of the object safety and dynamic dispatch
//! of the `Driver` trait, `ErasedDriver`, and `Box<AnyDriver>`.

use async_trait::async_trait;
use futures::executor::block_on;
use tv_rs_core::capability::{
    AppLaunch, Capability, CapabilitySet, DeviceHandle, InputSelect, KeyInput, PowerControl,
    TextInput, VolumeControl,
};
use tv_rs_core::command::{Command, CommandAck, CommandId};
use tv_rs_core::descriptor::{DeviceDescriptor, DeviceId, DeviceOs, DiscoveredTv, PowerState};
use tv_rs_core::driver::{AnyDriver, BoxedDeviceHandle, Driver, ErasedDriver};
use tv_rs_core::error::{ProtocolError, TransportError, TvError};
use tv_rs_core::observe::ObservationStream;
use tv_rs_core::types::TvVendor;

struct MockPower {
    state: PowerState,
}

#[async_trait]
impl PowerControl for MockPower {
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

struct ConcreteMockHandle {
    id: DeviceId,
    descriptor: DeviceDescriptor,
    capabilities: CapabilitySet,
    power: MockPower,
}

impl DeviceHandle for ConcreteMockHandle {
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

struct ConcreteMockDriver {
    vendor: TvVendor,
    capabilities: CapabilitySet,
    fail_connect: bool,
    fail_send: bool,
    fail_observe: bool,
    connected_target: Option<String>,
}

impl ConcreteMockDriver {
    fn new(vendor: TvVendor) -> Self {
        Self {
            vendor,
            capabilities: CapabilitySet::empty()
                .with(Capability::PowerState)
                .with(Capability::Key),
            fail_connect: false,
            fail_send: false,
            fail_observe: false,
            connected_target: None,
        }
    }
}

#[async_trait]
impl Driver for ConcreteMockDriver {
    type Handle = ConcreteMockHandle;

    fn vendor(&self) -> TvVendor {
        self.vendor
    }

    fn capabilities(&self) -> &CapabilitySet {
        &self.capabilities
    }

    async fn connect(&mut self, target: &DiscoveredTv) -> Result<Self::Handle, TvError> {
        if self.fail_connect {
            return Err(TvError::Transport(TransportError::Unreachable(
                "connection failed intentionally".to_string(),
            )));
        }
        self.connected_target = Some(target.ip.clone());
        let device_id = DeviceId::new(format!("{}-{}", self.vendor.as_str(), target.ip))
            .unwrap_or_else(|_| DeviceId::anonymous());
        let desc = DeviceDescriptor::new(device_id.clone(), DeviceOs::Unknown, &target.ip);
        Ok(ConcreteMockHandle {
            id: device_id,
            descriptor: desc,
            capabilities: self.capabilities,
            power: MockPower {
                state: PowerState::On,
            },
        })
    }

    async fn send(&mut self, cmd: Command) -> Result<CommandAck, TvError> {
        if self.fail_send {
            return Err(TvError::Protocol(ProtocolError::Malformed(
                "send failed intentionally".to_string(),
            )));
        }
        match cmd {
            Command::Custom { name, .. } if name == "unrecognized" => {
                // Return an ack with a message indicating unknown command
                Ok(CommandAck {
                    id: None,
                    message: Some("unrecognized command".to_string()),
                })
            }
            _ => Ok(CommandAck::with_id(CommandId::new())),
        }
    }

    async fn observe(&mut self) -> Result<ObservationStream, TvError> {
        if self.fail_observe {
            return Err(TvError::Transport(TransportError::Unreachable(
                "observe stream failed".to_string(),
            )));
        }
        Ok(ObservationStream::new())
    }
}

#[test]
fn test_object_safety_and_dynamic_dispatch() {
    block_on(async {
        let concrete_driver = ConcreteMockDriver::new(TvVendor::Samsung);
        let erased = ErasedDriver(concrete_driver);
        let mut boxed_driver: Box<AnyDriver> = Box::new(erased);

        // 1. Static query dynamic dispatch
        assert_eq!(boxed_driver.vendor(), TvVendor::Samsung);
        assert!(boxed_driver.capabilities().contains(Capability::PowerState));
        assert!(boxed_driver.capabilities().contains(Capability::Key));
        assert!(!boxed_driver.capabilities().contains(Capability::AppLaunch));

        // 2. Connect dynamic dispatch and handle erasure
        let target = DiscoveredTv::probed(TvVendor::Samsung, "192.168.1.55");
        let handle: BoxedDeviceHandle = boxed_driver
            .connect(&target)
            .await
            .expect("connect succeeds");

        assert_eq!(handle.id().as_str(), "samsung-192.168.1.55");
        assert_eq!(handle.descriptor().address, "192.168.1.55");
        let power_accessor = handle.as_power().expect("as_power supported");
        let power_state = power_accessor.power_state().await.expect("power state ok");
        assert_eq!(power_state, PowerState::On);

        // 3. Send dynamic dispatch
        let ack = boxed_driver
            .send(Command::Power(true))
            .await
            .expect("send succeeds");
        assert!(ack.id.is_some());
        assert_eq!(ack.message, None);

        // 4. Observe dynamic dispatch
        let stream = boxed_driver.observe().await.expect("observe succeeds");
        assert_eq!(format!("{stream:?}"), "ObservationStream { _private: () }");
    });
}

#[test]
fn test_heterogeneous_driver_vector_dispatch() {
    block_on(async {
        let mut registry: Vec<Box<AnyDriver>> = vec![
            Box::new(ErasedDriver(ConcreteMockDriver::new(TvVendor::Samsung))),
            Box::new(ErasedDriver(ConcreteMockDriver::new(TvVendor::Lg))),
            Box::new(ErasedDriver(ConcreteMockDriver::new(TvVendor::Roku))),
        ];

        assert_eq!(registry.len(), 3);
        let vendors: Vec<TvVendor> = registry.iter().map(|d| d.vendor()).collect();
        assert_eq!(
            vendors,
            vec![TvVendor::Samsung, TvVendor::Lg, TvVendor::Roku]
        );

        for driver in registry.iter_mut() {
            let target = DiscoveredTv::probed(driver.vendor(), "192.168.1.10");
            let handle = driver.connect(&target).await.expect("connects");
            assert!(handle.id().as_str().contains(driver.vendor().as_str()));

            let ack = driver
                .send(Command::Key(tv_rs_core::capability::Key::Power))
                .await
                .expect("sent");
            assert!(ack.id.is_some());
        }
    });
}

#[test]
fn test_driver_error_paths() {
    block_on(async {
        let mut driver_err = ConcreteMockDriver::new(TvVendor::Ir);
        driver_err.fail_connect = true;
        driver_err.fail_send = true;
        driver_err.fail_observe = true;

        let mut boxed: Box<AnyDriver> = Box::new(ErasedDriver(driver_err));
        let target = DiscoveredTv::probed(TvVendor::Ir, "10.0.0.1");

        let conn_res = boxed.connect(&target).await;
        match conn_res {
            Err(TvError::Transport(TransportError::Unreachable(msg))) => {
                assert_eq!(msg, "connection failed intentionally");
            }
            Err(e) => panic!("expected Transport Unreachable, got {e}"),
            Ok(_) => panic!("expected error, got Ok"),
        }

        let send_res = boxed.send(Command::Power(false)).await;
        match send_res {
            Err(TvError::Protocol(ProtocolError::Malformed(msg))) => {
                assert_eq!(msg, "send failed intentionally");
            }
            other => panic!("expected Protocol Malformed, got {other:?}"),
        }

        let obs_res = boxed.observe().await;
        match obs_res {
            Err(TvError::Transport(TransportError::Unreachable(msg))) => {
                assert_eq!(msg, "observe stream failed");
            }
            other => panic!("expected Transport Unreachable, got {other:?}"),
        }
    });
}

#[test]
fn test_unrecognized_command_ack_and_mock_stream() {
    block_on(async {
        let mut driver: Box<AnyDriver> =
            Box::new(ErasedDriver(ConcreteMockDriver::new(TvVendor::Philips)));

        let ack = driver
            .send(Command::Custom {
                name: "unrecognized".to_string(),
                payload: "raw-bytes".to_string(),
            })
            .await
            .expect("send unrecognized command ack");

        assert_eq!(ack.id, None);
        assert_eq!(ack.message.as_deref(), Some("unrecognized command"));

        let stream = driver.observe().await.expect("mock stream");
        assert_eq!(format!("{stream:?}"), "ObservationStream { _private: () }");
    });
}
