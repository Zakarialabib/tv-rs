//! Protocol buffer bindings for the Android TV Remote protocol v2.
//!
//! The bindings are **pre-generated and committed** (the same pattern
//! `tv-rs-cast` uses for `cast_channel.rs`): `prost-build` needs `protoc` on
//! the build host, and requiring it in every CI job and every contributor
//! checkout for two frozen `.proto` files buys nothing. Regenerate with
//!
//! ```text
//! protoc --prost_out=... # or: cargo run -p xtask regenerate-protos (when added)
//! ```
//!
//! only when `proto/polo.proto` or `proto/remotemessage.proto` changes — and
//! check the diff of these two files in the same commit.

#[allow(missing_docs)]
#[allow(clippy::all)]
pub mod polo {
    include!("proto/polo.rs");
}

#[allow(missing_docs)]
#[allow(clippy::all)]
pub mod remote {
    include!("proto/remote.rs");
}
