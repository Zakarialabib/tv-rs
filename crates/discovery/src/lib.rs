//! `tv-rs-discovery` — SSDP discovery and UPnP descriptor parsing.
//!
//! mDNS/Bonjour is *planned*, not implemented: [`DiscoveryMethod`] names it so the
//! enum does not need a breaking change later, but only SSDP runs today.
//!
//! This crate locates televisions on the local network before any control
//! traffic is sent. Phase 1 implements SSDP `M-SEARCH` over UDP multicast
//! (port 1900) and parsing of the resulting UPnP device descriptor XML.
//!
//! **Discovery needs no authentication.** SSDP is an unauthenticated LAN
//! multicast protocol: any device on the local subnet can be discovered and its
//! descriptor fetched over plain HTTP. mDNS/Bonjour and ARP are represented by
//! [`DiscoveryMethod`] but not implemented yet.
//!
//! # Example
//!
//! ```no_run
//! use tv_rs_discovery::SsdpScanner;
//!
//! # async fn example() -> Result<(), tv_rs_discovery::DiscoveryError> {
//! let scanner = SsdpScanner::new();
//!
//! for response in scanner.discover().await? {
//!     if let Some(location) = response.location {
//!         println!("{} -> {}", response.source, location);
//!     }
//! }
//! # Ok(())
//! # }
//! ```

pub mod client;
pub mod error;
pub mod payload;
pub mod protocol;
pub mod types;

pub use client::{build_msearch, parse_device_descriptor, parse_msearch_response, SsdpScanner};
pub use error::{DiscoveryError, Result};
pub use payload::{SsdpDeviceDescriptor, SsdpMsearchResponse};
pub use protocol::SsdpDiscovery;
pub use types::{
    DiscoveryMethod, MDNS_MULTICAST_IP, MDNS_UDP_PORT, SSDP_ALL_DEVICES, SSDP_MULTICAST_IP,
    SSDP_ROOT_DEVICE, SSDP_UDP_PORT,
};
