//! Driver registry aggregating all enabled protocol implementations.

pub use tv_rs_core::driver::{AnyDriver, ErasedDriver};
use tv_rs_core::types::TvVendor;
use tv_rs_core::descriptor::DiscoveredTv;

/// Returns all driver implementations enabled via Cargo feature flags.
#[must_use]
pub fn registry() -> Vec<Box<AnyDriver>> {
    #[allow(unused_mut)]
    let mut drivers: Vec<Box<AnyDriver>> = Vec::new();

    #[cfg(feature = "tizen")]
    {
        // Instantiated in TV-12
    }

    #[cfg(feature = "webos")]
    {
        // Instantiated in TV-12
    }

    #[cfg(feature = "roku")]
    {
        drivers.push(Box::new(ErasedDriver(crate::roku::RokuDriver::new())));
    }

    #[cfg(feature = "mdc")]
    {
        drivers.push(Box::new(ErasedDriver(crate::mdc::MdcDriver::new())));
    }

    #[cfg(feature = "sicp")]
    {
        drivers.push(Box::new(ErasedDriver(crate::sicp::SicpDriver::new())));
    }

    #[cfg(feature = "ir")]
    {
        drivers.push(Box::new(ErasedDriver(crate::ir::IrDriver::new())));
    }

    #[cfg(feature = "cec")]
    {
        // Instantiated in TV-14
    }

    #[cfg(feature = "cast")]
    {
        // Instantiated in TV-14
    }

    drivers
}

/// Dynamic, expanding registry supporting runtime plug-and-play driver registration.
#[derive(Default)]
pub struct DriverRegistry {
    drivers: Vec<Box<AnyDriver>>,
}

impl DriverRegistry {
    /// Creates an empty driver registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            drivers: Vec::new(),
        }
    }

    /// Creates a registry pre-populated with all compile-time enabled drivers.
    #[must_use]
    pub fn with_defaults() -> Self {
        Self {
            drivers: registry(),
        }
    }

    /// Registers an erased driver trait object.
    pub fn register(&mut self, driver: Box<AnyDriver>) {
        self.drivers.push(driver);
    }

    /// Registers a concrete driver, erasing its handle type into `Box<dyn DeviceHandle>`.
    pub fn register_driver<D>(&mut self, driver: D)
    where
        D: tv_rs_core::driver::Driver + 'static,
        D::Handle: 'static,
    {
        self.drivers.push(Box::new(ErasedDriver(driver)));
    }

    /// Finds a driver matching the given vendor.
    #[must_use]
    pub fn find_for_vendor(&self, vendor: TvVendor) -> Option<&AnyDriver> {
        self.drivers.iter().find(|d| d.vendor() == vendor).map(|b| &**b)
    }

    /// Finds a driver matching the given vendor (mutable).
    #[must_use]
    pub fn find_for_vendor_mut(&mut self, vendor: TvVendor) -> Option<&mut AnyDriver> {
        self.drivers.iter_mut().find(|d| d.vendor() == vendor).map(|b| &mut **b)
    }

    /// Finds a driver that can handle the discovered target.
    #[must_use]
    pub fn find_for_target(&self, target: &DiscoveredTv) -> Option<&AnyDriver> {
        self.find_for_vendor(target.vendor)
    }

    /// Returns a slice of all registered drivers.
    #[must_use]
    pub fn drivers(&self) -> &[Box<AnyDriver>] {
        &self.drivers
    }

    /// Returns a mutable slice of all registered drivers.
    pub fn drivers_mut(&mut self) -> &mut [Box<AnyDriver>] {
        &mut self.drivers
    }

    /// Consumes the registry and returns its drivers vector.
    #[must_use]
    pub fn into_drivers(self) -> Vec<Box<AnyDriver>> {
        self.drivers
    }

    /// Returns the number of registered drivers.
    #[must_use]
    pub fn len(&self) -> usize {
        self.drivers.len()
    }

    /// Returns true if no drivers are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.drivers.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_instantiates_and_queries() {
        let default_drivers = registry();
        let reg = DriverRegistry::with_defaults();
        assert_eq!(reg.len(), default_drivers.len());

        #[cfg(feature = "roku")]
        {
            assert!(reg.find_for_vendor(TvVendor::Roku).is_some());
        }
        #[cfg(feature = "mdc")]
        {
            assert!(reg.find_for_vendor(TvVendor::Mdc).is_some());
        }
        #[cfg(feature = "sicp")]
        {
            assert!(reg.find_for_vendor(TvVendor::Sicp).is_some());
        }
        #[cfg(feature = "ir")]
        {
            assert!(reg.find_for_vendor(TvVendor::Ir).is_some());
        }
    }

    #[test]
    fn extensible_registry_supports_runtime_registration() {
        let mut reg = DriverRegistry::new();
        assert!(reg.is_empty());

        #[cfg(feature = "roku")]
        {
            reg.register_driver(crate::roku::RokuDriver::new());
            assert_eq!(reg.len(), 1);
            assert!(reg.find_for_vendor(TvVendor::Roku).is_some());
        }
    }
}
