//! `tv-rs-tizen` (deprecated shim).
//!
//! Samsung Tizen driver functionality has consolidated into `tv-rs-drivers::tizen`.

#![allow(deprecated)]

#[deprecated(
    since = "0.8.0",
    note = "Consolidated into tv-rs-drivers; use tv_rs_drivers::tizen instead"
)]
pub use tv_rs_drivers::tizen::*;
