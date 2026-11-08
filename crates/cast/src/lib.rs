//! `tv-rs-cast` (deprecated shim).
//!
//! Google Cast driver functionality has consolidated into `tv-rs-drivers::cast`.

#![allow(deprecated)]

#[deprecated(
    since = "0.8.0",
    note = "Consolidated into tv-rs-drivers; use tv_rs_drivers::cast instead"
)]
pub use tv_rs_drivers::cast::*;
