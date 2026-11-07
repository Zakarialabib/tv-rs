//! Unified driver abstraction for television protocol backends.
//!
//! Defined per SPECDD §2.3: every protocol vendor implements [`Driver`], allowing
//! uniform connection, command dispatch, and event observation without vendor-specific
//! pattern matching in client code.

use async_trait::async_trait;

use crate::capability::{CapabilitySet, DeviceHandle};
use crate::command::{Command, CommandAck};
use crate::descriptor::DiscoveredTv;
use crate::error::TvError;
use crate::observe::ObservationStream;
use crate::types::TvVendor;

/// Uniform driver interface implemented by all protocol vendors.
///
/// Implementations must be thread-safe (`Send + Sync`).
#[async_trait]
pub trait Driver: Send + Sync {
    /// Associated handle type implementing the erased device handle.
    type Handle: DeviceHandle;

    /// Returns the vendor enum identifier targeted by this driver.
    fn vendor(&self) -> TvVendor;

    /// Returns the static capability set advertised by this driver.
    fn capabilities(&self) -> &CapabilitySet;

    /// Establishes a connection to the discovered TV target.
    async fn connect(&mut self, target: &DiscoveredTv) -> Result<Self::Handle, TvError>;

    /// Sends an imperative command to the device and waits for acknowledgement.
    async fn send(&mut self, cmd: Command) -> Result<CommandAck, TvError>;

    /// Opens an observation event stream for device state transitions.
    async fn observe(&mut self) -> Result<ObservationStream, TvError>;
}

/// An erased device handle boxed as a trait object.
pub type BoxedDeviceHandle = Box<dyn DeviceHandle>;

/// An object-safe type alias for drivers whose handle has been boxed into a trait object.
///
/// Enables heterogeneous storage in [`tv_rs_drivers::registry`](crate).
pub type AnyDriver = dyn Driver<Handle = BoxedDeviceHandle>;

/// Adapter that erases a concrete driver's handle type into a `Box<dyn DeviceHandle>`.
pub struct ErasedDriver<D>(pub D);

#[async_trait]
impl<D> Driver for ErasedDriver<D>
where
    D: Driver,
    D::Handle: 'static,
{
    type Handle = BoxedDeviceHandle;

    fn vendor(&self) -> TvVendor {
        self.0.vendor()
    }

    fn capabilities(&self) -> &CapabilitySet {
        self.0.capabilities()
    }

    async fn connect(&mut self, target: &DiscoveredTv) -> Result<Self::Handle, TvError> {
        let handle = self.0.connect(target).await?;
        Ok(Box::new(handle))
    }

    async fn send(&mut self, cmd: Command) -> Result<CommandAck, TvError> {
        self.0.send(cmd).await
    }

    async fn observe(&mut self) -> Result<ObservationStream, TvError> {
        self.0.observe().await
    }
}
