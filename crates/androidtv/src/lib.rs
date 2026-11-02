//! `tv-rs-androidtv` — Android TV / Google TV Remote protocol v2.
//!
//! Two fixed ports: `6467` (mTLS Polo pairing handshake) and `6466`
//! (protobuf `RemoteMessage` control). Device name and MAC parse from
//! the server certificate subject on the pairing port.
//!
//! # Example
//!
//! ```no_run
//! use tv_rs_androidtv::keys::Key;
//! use tv_rs_androidtv::AtrClient;
//!
//! # fn run() -> tv_rs_androidtv::Result<()> {
//! let mut client = AtrClient::new("192.168.1.20");
//! // Pair: read the 6-hex code from the TV screen (blocking handshake).
//! let identity = client.pair("tv-rs", "123456")?;
//! println!("Paired with {} ({})", identity.name, identity.mac);
//!
//! // Control: press Home.
//! let mut control = client.connect_control()?;
//! control.press_key(Key::Home)?;
//! # Ok(())
//! # }
//! ```
#![forbid(unsafe_code)]
#![allow(clippy::result_large_err)]

/// High-level client: TLS setup, Polo pairing, and control-session factory.
pub mod client;
/// Protobuf control client over port 6466.
pub mod control;
/// Error types.
pub mod error;
/// Remote key codes and the TvKeys catalog.
pub mod keys;
/// Polo pairing handshake (port 6467).
pub mod pairing;
/// Protocol buffer bindings (prost-generated, from `polo.proto` and
/// `remotemessage.proto`).
pub mod proto;

pub use client::{generate_client_cert, AtrClient};
pub use control::{ControlResponse, ControlSession};
pub use error::{AtrError, Result};
pub use keys::{Direction, Key};
pub use pairing::{parse_device_identity, DeviceIdentity};
