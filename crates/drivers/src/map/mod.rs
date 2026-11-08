//! Unified Error-to-Remedy mapping modules per vendor.

#[cfg(feature = "roku")]
pub mod roku;

#[cfg(feature = "ir")]
pub mod ir;

#[cfg(feature = "mdc")]
pub mod mdc;

#[cfg(feature = "sicp")]
pub mod sicp;
