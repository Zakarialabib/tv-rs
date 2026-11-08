//! `tv-rs-cec` (deprecated shim).
//!
//! HDMI-CEC driver functionality has consolidated into `tv-rs-drivers::cec`.

#![allow(deprecated)]

#[deprecated(
    since = "0.8.0",
    note = "Consolidated into tv-rs-drivers; use tv_rs_drivers::cec instead"
)]
pub use tv_rs_drivers::cec::*;
