//! `tv-rs-webos` (deprecated shim).
//!
//! LG webOS driver functionality has consolidated into `tv-rs-drivers::webos`.

#![allow(deprecated)]

#[deprecated(
    since = "0.8.0",
    note = "Consolidated into tv-rs-drivers; use tv_rs_drivers::webos instead"
)]
pub use tv_rs_drivers::webos::*;
