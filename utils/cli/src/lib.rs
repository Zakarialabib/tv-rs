//! `tv-rs` — the command-line front end for the [`tv_rs`] facade.
//!
//! This crate is the facade's first real consumer: it discovers televisions,
//! wakes them over Wake-on-LAN, and drives Samsung Tizen, LG webOS and Roku
//! devices through the facade's vendor dispatch. Layout is kept deliberately
//! small — a thin [`commands`] module performs I/O, a pure [`render`] module
//! turns facade data into text, and a single [`error::CliError`] carries the
//! process exit code. The binary (`src/main.rs`) only parses arguments and
//! delegates here.
//!
//! Commands:
//!
//! * `tv-rs discover [--timeout <SECONDS>]`
//! * `tv-rs wake <MAC> [--broadcast <IP:PORT>]`
//! * `tv-rs power-off <IP> --vendor <samsung|lg|roku>`
//! * `tv-rs key <IP> --vendor <samsung|roku> --key <KEY>`
//! * `tv-rs launch <IP> --vendor <samsung|lg|roku> --app <APP_ID>`
//!
//! # Example
//!
//! ```
//! use tv_rs::TvVendor;
//!
//! // The vendor vocabulary the CLI prints in its discovery table, and the
//! // control ports it reports, both come from the facade.
//! assert_eq!(TvVendor::Roku.as_str(), "roku");
//! assert_eq!(TvVendor::Samsung.control_port(), Some(8001));
//! ```

#![deny(missing_docs)]

pub mod cli;
pub mod commands;
pub mod error;
pub mod render;
