//! `tv-rs-discovery` — SSDP/mDNS discovery, UPnP descriptor parsing, and signage pairing.
//!
//! This crate locates televisions and digital signage players on the local network before any
//! control traffic is sent. It implements SSDP `M-SEARCH` over UDP multicast (port 1900), UPnP
//! descriptor parsing, mDNS/Bonjour service discovery (`MdnsScanner`/`MdnsAdvertiser`), and dynamic
//! QR code session token generation/validation for digital signage pairing (`SignagePairingSession`).
//!
//! **Discovery needs no authentication.** SSDP and mDNS are unauthenticated LAN
//! multicast protocols: any device on the local subnet can be discovered and its
//! descriptor fetched over plain HTTP.
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

#![forbid(unsafe_code)]
pub mod client;
pub mod error;
pub mod mdns;
pub mod pairing;
pub mod payload;
pub mod protocol;
pub mod types;

pub use client::{build_msearch, parse_device_descriptor, parse_msearch_response, SsdpScanner};
pub use error::{DiscoveryError, Result};
pub use mdns::{
    MdnsAdvertiser, MdnsScanner, MdnsServiceRecord, MDNS_SERVICE_TYPE_SIGNAGE, MDNS_SERVICE_TYPE_TV,
};
pub use pairing::{
    validate_handshake, SignageHandshakeRequest, SignageHandshakeResponse, SignagePairingPayload,
    SignagePairingSession, DEFAULT_SESSION_TTL_SECS, SIGNAGE_PAIRING_URI_SCHEME,
};
pub use payload::{SsdpDeviceDescriptor, SsdpMsearchResponse};
pub use protocol::SsdpDiscovery;
pub use types::{
    DiscoveryMethod, MDNS_MULTICAST_IP, MDNS_UDP_PORT, SSDP_ALL_DEVICES, SSDP_MULTICAST_IP,
    SSDP_ROOT_DEVICE, SSDP_UDP_PORT,
};
