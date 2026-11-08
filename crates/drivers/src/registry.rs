//! Driver registry aggregating all enabled protocol implementations.

use std::sync::OnceLock;
pub use tv_rs_core::driver::{AnyDriver, ErasedDriver};
use tv_rs_core::types::TvVendor;
use tv_rs_core::descriptor::DiscoveredTv;
use tv_rs_core::capability::{Capability, CapabilityRegistry, CapabilitySet};

static DEFAULT_REGISTRY: OnceLock<DriverRegistry> = OnceLock::new();

/// Returns a shared, lazily-initialized reference to the default compile-time driver registry.
#[must_use]
pub fn default_registry() -> &'static DriverRegistry {
    DEFAULT_REGISTRY.get_or_init(DriverRegistry::with_defaults)
}

/// Returns all driver implementations enabled via Cargo feature flags.
#[must_use]
pub fn registry() -> Vec<Box<AnyDriver>> {
    #[allow(unused_mut)]
    let mut drivers: Vec<Box<AnyDriver>> = Vec::new();

    #[cfg(feature = "tizen")]
    {
        drivers.push(Box::new(ErasedDriver(crate::tizen::TizenDriver::new())));
    }

    #[cfg(feature = "webos")]
    {
        drivers.push(Box::new(ErasedDriver(crate::webos::WebOsDriver::new())));
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
        drivers.push(Box::new(ErasedDriver(crate::cec::CecDriver::new())));
    }

    #[cfg(feature = "cast")]
    {
        drivers.push(Box::new(ErasedDriver(crate::cast::CastDriver::new())));
    }

    drivers
}

fn matches_vendor(driver_vendor: TvVendor, query_vendor: TvVendor) -> bool {
    if driver_vendor == query_vendor {
        return true;
    }
    matches!(
        (driver_vendor, query_vendor),
        (TvVendor::Samsung, TvVendor::Tizen)
            | (TvVendor::Tizen, TvVendor::Samsung)
            | (TvVendor::Lg, TvVendor::WebOs)
            | (TvVendor::WebOs, TvVendor::Lg)
    )
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
        self.drivers
            .iter()
            .find(|d| matches_vendor(d.vendor(), vendor))
            .map(|b| &**b)
    }

    /// Finds a driver matching the given vendor (mutable).
    #[must_use]
    pub fn find_for_vendor_mut(&mut self, vendor: TvVendor) -> Option<&mut AnyDriver> {
        self.drivers
            .iter_mut()
            .find(|d| matches_vendor(d.vendor(), vendor))
            .map(|b| &mut **b)
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

    /// Returns the capabilities advertised by the driver matching `vendor`, if registered.
    #[must_use]
    pub fn capabilities_for_vendor(&self, vendor: TvVendor) -> Option<CapabilitySet> {
        self.find_for_vendor(vendor).map(CapabilitySet::from_driver)
    }

    /// Computes the union of capabilities across all currently registered drivers.
    #[must_use]
    pub fn aggregate_capabilities(&self) -> CapabilitySet {
        let mut union = CapabilitySet::empty();
        for driver in &self.drivers {
            union.extend(&CapabilitySet::from_driver(&**driver));
        }
        union
    }

    /// Constructs a `CapabilityRegistry` snapshot from all registered drivers.
    #[must_use]
    pub fn build_capability_registry(&self) -> CapabilityRegistry {
        let mut cap_reg = CapabilityRegistry::new();
        for driver in &self.drivers {
            cap_reg.register_driver(&**driver);
        }
        cap_reg
    }

    /// Returns a list of all vendors represented in this driver registry.
    #[must_use]
    pub fn vendors(&self) -> Vec<TvVendor> {
        self.drivers.iter().map(|d| d.vendor()).collect()
    }

    /// Returns all registered drivers that advertise `capability`.
    #[must_use]
    pub fn find_by_capability(&self, capability: Capability) -> Vec<&AnyDriver> {
        self.drivers
            .iter()
            .filter(|d| d.capabilities().contains(capability))
            .map(|b| &**b)
            .collect()
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

        #[cfg(feature = "tizen")]
        {
            assert!(reg.find_for_vendor(TvVendor::Samsung).is_some());
            assert!(reg.find_for_vendor(TvVendor::Tizen).is_some());
        }
        #[cfg(feature = "webos")]
        {
            assert!(reg.find_for_vendor(TvVendor::Lg).is_some());
            assert!(reg.find_for_vendor(TvVendor::WebOs).is_some());
        }
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
        #[cfg(feature = "cec")]
        {
            assert!(reg.find_for_vendor(TvVendor::Cec).is_some());
        }
        #[cfg(feature = "cast")]
        {
            assert!(reg.find_for_vendor(TvVendor::GoogleCast).is_some());
        }
    }

    #[test]
    fn extensible_registry_supports_runtime_registration() {
        #[allow(unused_mut)]
        let mut reg = DriverRegistry::new();
        assert!(reg.is_empty());

        #[cfg(feature = "roku")]
        {
            reg.register_driver(crate::roku::RokuDriver::new());
            assert_eq!(reg.len(), 1);
            assert!(reg.find_for_vendor(TvVendor::Roku).is_some());
        }
    }

    #[test]
    fn default_registry_and_capability_aggregation() {
        let def = default_registry();
        assert_eq!(def.len(), registry().len());

        let agg = def.aggregate_capabilities();
        assert!(!agg.is_empty());

        let cap_reg = def.build_capability_registry();
        assert_eq!(cap_reg.len(), def.len());

        let power_drivers = def.find_by_capability(Capability::PowerState);
        assert!(!power_drivers.is_empty());
    }
}
