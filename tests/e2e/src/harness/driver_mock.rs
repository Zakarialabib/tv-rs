//! Driver Trait, Mock Implementations, Registry, and Conformance Harness.

use async_trait::async_trait;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TvError {
    #[error("routing failed: unreachable")]
    RoutingUnreachable,
    #[error("routing failed: timeout")]
    RoutingTimeout,
    #[error("authentication failed")]
    AuthenticationFailed,
    #[error("capability missing")]
    CapabilityMissing,
    #[error("standby refusal")]
    StandbyRefusal,
    #[error("unsupported: {0:?}")]
    Unsupported(Capability),
    #[error("no control client for {address}: {reason}")]
    UnsupportedDevice { address: String, reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct CommandId(String);

impl CommandId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DeviceId(String);

impl DeviceId {
    pub fn new(id: impl Into<String>) -> Result<Self, String> {
        let s = id.into();
        if s.is_empty() {
            Err("Empty device ID".to_string())
        } else {
            Ok(Self(s))
        }
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum TvVendor {
    Samsung,
    Lg,
    Roku,
    Mdc,
    Sicp,
    SonyIr,
    Cec,
    GoogleCast,
    GenericDiscovery,
    GenericPower,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Capability {
    PowerOn,
    PowerOff,
    PowerQuery,
    VolumeControl,
    MuteToggle,
    KeyDispatch,
    AppLaunch,
    MediaControl,
    MediaStatus,
    InputSourceSelection,
    VideoWallControl,
    ObservationPush,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct CapabilitySet {
    capabilities: HashSet<Capability>,
}

impl CapabilitySet {
    pub fn new() -> Self {
        Self {
            capabilities: HashSet::new(),
        }
    }

    pub fn with_capabilities(caps: impl IntoIterator<Item = Capability>) -> Self {
        Self {
            capabilities: caps.into_iter().collect(),
        }
    }

    pub fn insert(&mut self, cap: Capability) {
        self.capabilities.insert(cap);
    }

    pub fn contains(&self, cap: &Capability) -> bool {
        self.capabilities.contains(cap)
    }

    pub fn is_empty(&self) -> bool {
        self.capabilities.is_empty()
    }

    pub fn len(&self) -> usize {
        self.capabilities.len()
    }

    pub fn union(&self, other: &Self) -> Self {
        Self {
            capabilities: self.capabilities.union(&other.capabilities).copied().collect(),
        }
    }

    pub fn iter(&self) -> std::collections::hash_set::Iter<'_, Capability> {
        self.capabilities.iter()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DiscoveredTv {
    pub id: String,
    pub vendor: TvVendor,
    pub ip: String,
    pub port: u16,
    pub friendly_name: String,
    pub mac: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Command {
    PowerOn,
    PowerOff,
    SendKey(String),
    LaunchApp(String),
    SetVolume(u8),
    Mute(bool),
    SetInput(String),
    SetVideoWall { rows: u8, cols: u8, row: u8, col: u8 },
    MediaStatus,
    MediaPlay,
    MediaPause,
    MediaStop,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CommandAck {
    pub success: bool,
    pub command: Command,
    pub response_data: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TvEvent {
    PowerStateChanged(bool),
    VolumeChanged(u8),
    MuteChanged(bool),
    AppOpened(String),
    InputChanged(String),
    MediaPlaybackChanged(String),
    DeviceDisconnected,
}

pub struct ObservationStream {
    receiver: broadcast::Receiver<TvEvent>,
}

impl ObservationStream {
    pub fn new(receiver: broadcast::Receiver<TvEvent>) -> Self {
        Self { receiver }
    }

    pub async fn recv(&mut self) -> Result<TvEvent, broadcast::error::RecvError> {
        self.receiver.recv().await
    }
}

#[async_trait]
pub trait DriverHandle: Send + Sync {
    fn device_id(&self) -> &str;
    async fn disconnect(&mut self) -> Result<(), TvError>;
    fn is_connected(&self) -> bool;
}

#[derive(Debug, Clone)]
pub struct MockHandle {
    pub id: String,
    pub connected: Arc<Mutex<bool>>,
}

#[async_trait]
impl DriverHandle for MockHandle {
    fn device_id(&self) -> &str {
        &self.id
    }

    async fn disconnect(&mut self) -> Result<(), TvError> {
        let mut conn = self.connected.lock().await;
        *conn = false;
        Ok(())
    }

    fn is_connected(&self) -> bool {
        // Fast sync check
        if let Ok(guard) = self.connected.try_lock() {
            *guard
        } else {
            true
        }
    }
}

#[async_trait]
pub trait Driver: Send + Sync {
    fn vendor(&self) -> TvVendor;
    fn capabilities(&self) -> &CapabilitySet;
    async fn connect(&mut self, target: &DiscoveredTv) -> Result<MockHandle, TvError>;
    async fn send(&mut self, cmd: Command) -> Result<CommandAck, TvError>;
    async fn observe(&mut self) -> Result<ObservationStream, TvError>;
}

// -------------------------------------------------------------
// Mock Driver Base Implementation
// -------------------------------------------------------------
pub struct BaseMockDriver {
    pub vendor: TvVendor,
    pub capabilities: CapabilitySet,
    pub connected: Arc<Mutex<bool>>,
    pub tx: broadcast::Sender<TvEvent>,
    pub command_history: Arc<Mutex<Vec<Command>>>,
}

impl BaseMockDriver {
    pub fn new(vendor: TvVendor, caps: &[Capability]) -> Self {
        let (tx, _) = broadcast::channel(64);
        Self {
            vendor,
            capabilities: CapabilitySet::with_capabilities(caps.iter().copied()),
            connected: Arc::new(Mutex::new(false)),
            tx,
            command_history: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn emit_event(&self, event: TvEvent) -> Result<usize, broadcast::error::SendError<TvEvent>> {
        self.tx.send(event)
    }

    pub async fn history(&self) -> Vec<Command> {
        self.command_history.lock().await.clone()
    }
}

#[async_trait]
impl Driver for BaseMockDriver {
    fn vendor(&self) -> TvVendor {
        self.vendor
    }

    fn capabilities(&self) -> &CapabilitySet {
        &self.capabilities
    }

    async fn connect(&mut self, target: &DiscoveredTv) -> Result<MockHandle, TvError> {
        if target.ip.is_empty() || target.ip == "0.0.0.0" {
            return Err(TvError::RoutingUnreachable);
        }
        let mut conn = self.connected.lock().await;
        *conn = true;
        Ok(MockHandle {
            id: target.id.clone(),
            connected: self.connected.clone(),
        })
    }

    async fn send(&mut self, cmd: Command) -> Result<CommandAck, TvError> {
        let mut hist = self.command_history.lock().await;
        hist.push(cmd.clone());

        let req_cap = match &cmd {
            Command::PowerOn => Capability::PowerOn,
            Command::PowerOff => Capability::PowerOff,
            Command::SendKey(_) => Capability::KeyDispatch,
            Command::LaunchApp(_) => Capability::AppLaunch,
            Command::SetVolume(_) => Capability::VolumeControl,
            Command::Mute(_) => Capability::MuteToggle,
            Command::SetInput(_) => Capability::InputSourceSelection,
            Command::SetVideoWall { .. } => Capability::VideoWallControl,
            Command::MediaStatus => Capability::MediaStatus,
            Command::MediaPlay | Command::MediaPause | Command::MediaStop => Capability::MediaControl,
        };

        if !self.capabilities.contains(&req_cap) {
            return Err(TvError::CapabilityMissing);
        }

        Ok(CommandAck {
            success: true,
            command: cmd,
            response_data: Some("OK".to_string()),
        })
    }

    async fn observe(&mut self) -> Result<ObservationStream, TvError> {
        if !self.capabilities.contains(&Capability::ObservationPush) {
            return Err(TvError::CapabilityMissing);
        }
        Ok(ObservationStream::new(self.tx.subscribe()))
    }
}

// -------------------------------------------------------------
// Preconfigured Mock Drivers for the 10 consolidated targets
// -------------------------------------------------------------
pub fn create_mock_tizen_driver() -> BaseMockDriver {
    BaseMockDriver::new(
        TvVendor::Samsung,
        &[
            Capability::PowerOn,
            Capability::PowerOff,
            Capability::KeyDispatch,
            Capability::AppLaunch,
            Capability::VolumeControl,
            Capability::MuteToggle,
            Capability::ObservationPush,
        ],
    )
}

pub fn create_mock_webos_driver() -> BaseMockDriver {
    BaseMockDriver::new(
        TvVendor::Lg,
        &[
            Capability::PowerOn,
            Capability::PowerOff,
            Capability::KeyDispatch,
            Capability::AppLaunch,
            Capability::VolumeControl,
            Capability::MuteToggle,
            Capability::InputSourceSelection,
            Capability::ObservationPush,
        ],
    )
}

pub fn create_mock_roku_driver() -> BaseMockDriver {
    BaseMockDriver::new(
        TvVendor::Roku,
        &[
            Capability::PowerOn,
            Capability::PowerOff,
            Capability::KeyDispatch,
            Capability::AppLaunch,
            Capability::InputSourceSelection,
            Capability::ObservationPush,
        ],
    )
}

pub fn create_mock_mdc_driver() -> BaseMockDriver {
    BaseMockDriver::new(
        TvVendor::Mdc,
        &[
            Capability::PowerOn,
            Capability::PowerOff,
            Capability::PowerQuery,
            Capability::VolumeControl,
            Capability::MuteToggle,
            Capability::InputSourceSelection,
            Capability::VideoWallControl,
            Capability::ObservationPush,
        ],
    )
}

pub fn create_mock_sicp_driver() -> BaseMockDriver {
    BaseMockDriver::new(
        TvVendor::Sicp,
        &[
            Capability::PowerOn,
            Capability::PowerOff,
            Capability::VolumeControl,
            Capability::InputSourceSelection,
        ],
    )
}

pub fn create_mock_ir_driver() -> BaseMockDriver {
    BaseMockDriver::new(
        TvVendor::SonyIr,
        &[
            Capability::PowerOn,
            Capability::PowerOff,
            Capability::KeyDispatch,
            Capability::VolumeControl,
        ],
    )
}

pub fn create_mock_cec_driver() -> BaseMockDriver {
    BaseMockDriver::new(
        TvVendor::Cec,
        &[
            Capability::PowerOn,
            Capability::PowerOff,
            Capability::InputSourceSelection,
            Capability::VolumeControl,
        ],
    )
}

pub fn create_mock_cast_driver() -> BaseMockDriver {
    BaseMockDriver::new(
        TvVendor::GoogleCast,
        &[
            Capability::AppLaunch,
            Capability::MediaControl,
            Capability::MediaStatus,
            Capability::VolumeControl,
            Capability::ObservationPush,
        ],
    )
}

pub fn create_mock_discovery_driver() -> BaseMockDriver {
    BaseMockDriver::new(
        TvVendor::GenericDiscovery,
        &[Capability::ObservationPush],
    )
}

pub fn create_mock_power_driver() -> BaseMockDriver {
    BaseMockDriver::new(
        TvVendor::GenericPower,
        &[Capability::PowerOn, Capability::PowerOff, Capability::PowerQuery],
    )
}

// -------------------------------------------------------------
// Driver Registry & Conformance Harness
// -------------------------------------------------------------
pub struct MockDriverRegistry {
    drivers: HashMap<TvVendor, Box<dyn Driver>>,
}

impl MockDriverRegistry {
    pub fn new() -> Self {
        Self {
            drivers: HashMap::new(),
        }
    }

    pub fn register(&mut self, driver: Box<dyn Driver>) {
        self.drivers.insert(driver.vendor(), driver);
    }

    pub fn get_mut(&mut self, vendor: &TvVendor) -> Option<&mut Box<dyn Driver>> {
        self.drivers.get_mut(vendor)
    }

    pub fn get(&self, vendor: &TvVendor) -> Option<&Box<dyn Driver>> {
        self.drivers.get(vendor)
    }

    pub fn vendors(&self) -> Vec<TvVendor> {
        self.drivers.keys().copied().collect()
    }

    pub fn len(&self) -> usize {
        self.drivers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.drivers.is_empty()
    }

    pub fn aggregate_capabilities(&self) -> CapabilitySet {
        let mut set = CapabilitySet::new();
        for driver in self.drivers.values() {
            set = set.union(driver.capabilities());
        }
        set
    }

    pub fn full_default_registry() -> Self {
        let mut reg = Self::new();
        reg.register(Box::new(create_mock_tizen_driver()));
        reg.register(Box::new(create_mock_webos_driver()));
        reg.register(Box::new(create_mock_roku_driver()));
        reg.register(Box::new(create_mock_mdc_driver()));
        reg.register(Box::new(create_mock_sicp_driver()));
        reg.register(Box::new(create_mock_ir_driver()));
        reg.register(Box::new(create_mock_cec_driver()));
        reg.register(Box::new(create_mock_cast_driver()));
        reg.register(Box::new(create_mock_discovery_driver()));
        reg.register(Box::new(create_mock_power_driver()));
        reg
    }
}

pub struct ConformanceHarness;

impl ConformanceHarness {
    pub fn verify_orphaned_capabilities(
        driver: &dyn Driver,
        conformance_cases: &[Capability],
    ) -> Result<(), String> {
        let cases_set: HashSet<Capability> = conformance_cases.iter().copied().collect();
        let mut orphaned = Vec::new();

        for cap in driver.capabilities().iter() {
            if !cases_set.contains(cap) {
                orphaned.push(format!("{:?}", cap));
            }
        }

        if !orphaned.is_empty() {
            return Err(format!(
                "Driver {:?} has orphaned capabilities without conformance cases: {:?}",
                driver.vendor(),
                orphaned
            ));
        }

        Ok(())
    }
}
