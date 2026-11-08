//! Consolidated protocol-vendor drivers for `tv-rs`.
//!
//! Provides feature-gated implementations of the [`tv_rs_core::Driver`] trait
//! across Samsung Tizen, LG webOS, Roku, Samsung MDC, Philips SICP, Sony IRCC,
//! HDMI-CEC, and Google Cast.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod registry;
pub use registry::{registry, AnyDriver, DriverRegistry};

pub mod map;

#[cfg(feature = "discovery")]
/// Discovery subsystem for UPnP, SSDP, and vendor-specific probes.
pub mod discovery;

#[cfg(feature = "power")]
/// Power control subsystem (Wake-on-LAN and TCP power probes).
pub mod power;

#[cfg(feature = "tizen")]
/// Samsung Tizen consumer television driver.
pub mod tizen {}

#[cfg(feature = "webos")]
/// LG webOS television driver.
pub mod webos {}

#[cfg(feature = "roku")]
/// Roku External Control Protocol (ECP) driver.
pub mod roku;

#[cfg(feature = "mdc")]
/// Samsung Multiple Display Control (MDC) commercial panel driver.
pub mod mdc;

#[cfg(feature = "sicp")]
/// Philips SICP commercial display driver.
pub mod sicp;

#[cfg(feature = "ir")]
/// Universal IR remote control and scanner driver.
pub mod ir;

#[cfg(feature = "cec")]
/// HDMI-CEC driver.
pub mod cec {}

#[cfg(feature = "cast")]
/// Google Cast and Chromecast driver.
pub mod cast {}
