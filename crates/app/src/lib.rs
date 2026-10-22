//! Package a supplied web/wasm payload as an installable TV app.
//!
//! A television app is a web app in a wrapper: a directory of files plus a manifest
//! the platform reads to decide whether to install it, what to call it in the app
//! list, and — on Tizen — whether to deliver remote-control key events to it at all.
//! This crate writes that wrapper around a payload you already have.
//!
//! **What it is not.** It does not play content, schedule anything or talk to a CMS.
//! The payload is yours; this crate's job ends when the platform's tooling can take
//! over. That boundary is what makes it usable by more than one application, and it is
//! the boundary the handoff that requested this crate drew.
//!
//! # How it fits together
//!
//! ```text
//! payload/            your web app: index.html, js, wasm — untouched
//!   + tv-bridge.js    generated: maps remote keys onto your payload's key_event()
//!   + config.xml      generated (Tizen) or appinfo.json (webOS)
//! = dist/<id>/        the bundle
//!   dist/<id>.wgt     the same bundle as a ZIP, for a Tizen TV
//! ```
//!
//! The payload interface is deliberately tiny, and every call is optional: a payload
//! that only implements `key_event(name, value)` is a complete payload.
//!
//! | the bridge calls | when |
//! |---|---|
//! | `boot()` | once, when the document is ready |
//! | `tick(now_ms)` | every animation frame |
//! | `key_event(name, value)` | on a remote key: `value` is `1` held, `0` released |
//! | `dispatch(json)` | when the app calls `window.tvBridge.send(json)` |
//! | `snapshot()` | when the app calls `window.tvBridge.snapshot()` |
//!
//! # Packaging
//!
//! ```no_run
//! use std::path::Path;
//! use tv_rs_app::{package, AppDescriptor, PackageRequest, Target};
//!
//! let descriptor = AppDescriptor::new("com.example.wall", "Lobby wall", "1.0.0")
//!     .with_vendor("Example Signage")
//!     .with_icon("icon.png");
//!
//! let request = PackageRequest::new(
//!     &descriptor,
//!     Path::new("payload"),
//!     Target::Tizen,
//!     Path::new("dist"),
//! );
//! let artifact = package(&request)?;
//! println!("wrote {}", artifact.path().display());
//! # Ok::<(), tv_rs_app::AppError>(())
//! ```
//!
//! # What each platform still needs
//!
//! This crate produces unsigned artifacts on purpose — signing belongs to the
//! operator, and installing belongs to the panel:
//!
//! - **Tizen**: `dist/<id>.wgt` is a ZIP with `config.xml` at its root, exactly what
//!   `tizen package` signs. Install it on a development TV over the developer
//!   connection, or sign it for retail.
//! - **webOS**: the app directory is complete, but an `.ipk` is an `ar` archive with a
//!   platform control file. [`package`] returns the app directory and
//!   [`ipk_command`] returns the `ares-package` invocation instead of fabricating one.
//! - **Web**: the bundle directory is served as-is.
//!
//! # Where the protocol facts live
//!
//! The manifest fields and the key names are documented in
//! [`docs/01-playbook.md`](https://github.com/Zakarialabib/tv-rs/blob/main/docs/01-playbook.md)
//! §3 (Tizen) and §4 (webOS), and each one is explained where it is set — in
//! `manifest.rs` and `keys.rs`.

mod bridge;
mod bundle;
mod descriptor;
mod error;
mod keys;
mod manifest;
mod target;
mod zip;

pub use bridge::{bridge_js, bridge_js_with, index_html, BridgeOptions, DEFAULT_PAYLOAD_GLOBAL};
pub use bundle::Bundle;
pub use descriptor::AppDescriptor;
pub use error::AppError;
pub use keys::Key;
pub use manifest::{tizen_config_xml, webos_appinfo_json, TIZEN_REQUIRED_VERSION};
pub use target::{ipk_command, package, Artifact, PackageRequest, Target};
