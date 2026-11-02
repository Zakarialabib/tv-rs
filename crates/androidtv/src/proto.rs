//! Protocol buffer bindings for the Android TV Remote protocol v2.

#[allow(missing_docs)]
#[allow(clippy::all)]
pub mod polo {
    include!(concat!(env!("OUT_DIR"), "/polo.wire.protobuf.rs"));
}

#[allow(missing_docs)]
#[allow(clippy::all)]
pub mod remote {
    include!(concat!(env!("OUT_DIR"), "/remote.rs"));
}
