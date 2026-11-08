//! `tv-rs-ir` (deprecated shim).
//!
//! Consolidated into `tv-rs-drivers`; use `tv_rs_drivers::ir` instead.

#![allow(deprecated)]

#[deprecated(
    since = "0.8.0",
    note = "Consolidated into tv-rs-drivers; use tv_rs_drivers::ir instead"
)]
pub use tv_rs_drivers::ir::*;
