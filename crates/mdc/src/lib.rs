//! `tv-rs-mdc` (deprecated shim).
//!
//! This crate has been consolidated into `tv-rs-drivers` under the `mdc` feature.
//! Please migrate dependencies to `tv-rs-drivers` with feature `mdc`.

#![allow(deprecated)]

#[deprecated(
    since = "0.8.0",
    note = "tv-rs-mdc is consolidated into tv-rs-drivers; use tv_rs_drivers::mdc instead"
)]
pub use tv_rs_drivers::mdc::*;
