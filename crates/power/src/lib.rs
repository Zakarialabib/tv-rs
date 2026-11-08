//! `tv-rs-power` (deprecated shim).
//!
//! This crate has been consolidated into `tv-rs-drivers` under the `power` feature.
//! Please migrate dependencies to `tv-rs-drivers` with `features = ["power"]` or
//! import via the facade `tv-rs::power`.

#![allow(deprecated)]

#[deprecated(since = "0.7.1", note = "Moved to tv_rs_drivers::power::client")]
pub use tv_rs_drivers::power::client;
#[deprecated(since = "0.7.1", note = "Moved to tv_rs_drivers::power::error")]
pub use tv_rs_drivers::power::error;
#[deprecated(since = "0.7.1", note = "Moved to tv_rs_drivers::power::payload")]
pub use tv_rs_drivers::power::payload;
#[deprecated(since = "0.7.1", note = "Moved to tv_rs_drivers::power::protocol")]
pub use tv_rs_drivers::power::protocol;
#[deprecated(since = "0.7.1", note = "Moved to tv_rs_drivers::power::types")]
pub use tv_rs_drivers::power::types;

#[deprecated(since = "0.7.1", note = "Moved to tv_rs_drivers::power::*")]
pub use tv_rs_drivers::power::*;
