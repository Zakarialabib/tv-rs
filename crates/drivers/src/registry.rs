//! Driver registry aggregating all enabled protocol implementations.

pub use tv_rs_core::driver::AnyDriver;

/// Returns all driver implementations enabled via Cargo feature flags.
#[must_use]
pub fn registry() -> Vec<Box<AnyDriver>> {
    let mut _drivers: Vec<Box<AnyDriver>> = Vec::new();

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
        // Instantiated in TV-13
    }

    #[cfg(feature = "mdc")]
    {
        // Instantiated in TV-13
    }

    #[cfg(feature = "sicp")]
    {
        // Instantiated in TV-13
    }

    #[cfg(feature = "ir")]
    {
        // Instantiated in TV-13
    }

    #[cfg(feature = "cec")]
    {
        // Instantiated in TV-14
    }

    #[cfg(feature = "cast")]
    {
        // Instantiated in TV-14
    }

    _drivers
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_returns_vector() {
        let drivers = registry();
        // In Wave 0 (skeleton), concrete driver implementations are registered in subsequent waves
        assert!(drivers.is_empty());
    }
}
