//! Build script for compiling Google Cast protobuf definitions.
fn main() {
    prost_build::compile_protos(&["proto/cast_channel.proto"], &["proto/"]).unwrap();
}
