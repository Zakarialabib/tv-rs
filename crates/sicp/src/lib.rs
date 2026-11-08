//! `tv-rs-sicp` (deprecated shim).
//!
//! This crate has been consolidated into `tv-rs-drivers` under the `sicp` feature.
//! Please migrate dependencies to `tv-rs-drivers` with feature `sicp`.

#![allow(deprecated)]

#[deprecated(
    since = "0.8.0",
    note = "tv-rs-sicp is consolidated into tv-rs-drivers; use tv_rs_drivers::sicp instead"
)]
pub use tv_rs_drivers::sicp::*;
