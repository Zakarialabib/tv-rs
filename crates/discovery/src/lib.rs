//! `tv-rs-discovery` (deprecated shim).
//!
//! This crate has been consolidated into `tv-rs-drivers` under the `discovery` feature.
//! Please migrate dependencies to `tv-rs-drivers` with `features = ["discovery"]` or
//! import via the facade `tv-rs::discovery`.

#![allow(deprecated)]

#[deprecated(since = "0.7.1", note = "Moved to tv_rs_drivers::discovery::adb")]
pub use tv_rs_drivers::discovery::adb;
#[deprecated(since = "0.7.1", note = "Moved to tv_rs_drivers::discovery::bluetooth")]
pub use tv_rs_drivers::discovery::bluetooth;
#[deprecated(since = "0.7.1", note = "Moved to tv_rs_drivers::discovery::cast")]
pub use tv_rs_drivers::discovery::cast;
#[deprecated(since = "0.7.1", note = "Moved to tv_rs_drivers::discovery::cec_topology")]
pub use tv_rs_drivers::discovery::cec_topology;
#[deprecated(since = "0.7.1", note = "Moved to tv_rs_drivers::discovery::client")]
pub use tv_rs_drivers::discovery::client;
#[deprecated(since = "0.7.1", note = "Moved to tv_rs_drivers::discovery::error")]
pub use tv_rs_drivers::discovery::error;
#[deprecated(since = "0.7.1", note = "Moved to tv_rs_drivers::discovery::fingerprint")]
pub use tv_rs_drivers::discovery::fingerprint;
#[deprecated(since = "0.7.1", note = "Moved to tv_rs_drivers::discovery::http_display")]
pub use tv_rs_drivers::discovery::http_display;
#[deprecated(since = "0.7.1", note = "Moved to tv_rs_drivers::discovery::mdns")]
pub use tv_rs_drivers::discovery::mdns;
#[deprecated(since = "0.7.1", note = "Moved to tv_rs_drivers::discovery::pairing")]
pub use tv_rs_drivers::discovery::pairing;
#[deprecated(since = "0.7.1", note = "Moved to tv_rs_drivers::discovery::payload")]
pub use tv_rs_drivers::discovery::payload;
#[deprecated(since = "0.7.1", note = "Moved to tv_rs_drivers::discovery::probe")]
pub use tv_rs_drivers::discovery::probe;
#[deprecated(since = "0.7.1", note = "Moved to tv_rs_drivers::discovery::protocol")]
pub use tv_rs_drivers::discovery::protocol;
#[deprecated(since = "0.7.1", note = "Moved to tv_rs_drivers::discovery::registry")]
pub use tv_rs_drivers::discovery::registry;
#[deprecated(since = "0.7.1", note = "Moved to tv_rs_drivers::discovery::source")]
pub use tv_rs_drivers::discovery::source;
#[deprecated(since = "0.7.1", note = "Moved to tv_rs_drivers::discovery::types")]
pub use tv_rs_drivers::discovery::types;
#[deprecated(since = "0.7.1", note = "Moved to tv_rs_drivers::discovery::universal")]
pub use tv_rs_drivers::discovery::universal;

#[deprecated(since = "0.7.1", note = "Moved to tv_rs_drivers::discovery::*")]
pub use tv_rs_drivers::discovery::*;
