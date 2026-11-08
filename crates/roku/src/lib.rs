//! `tv-rs-roku` (deprecated shim).
//!
//! Consolidated into `tv-rs-drivers`; use `tv_rs_drivers::roku` instead.

#![allow(deprecated)]

#[deprecated(
    since = "0.8.0",
    note = "Consolidated into tv-rs-drivers; use tv_rs_drivers::roku instead"
)]
pub use tv_rs_drivers::roku::*;
