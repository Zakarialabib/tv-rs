//! Prost codegen for the ATR v2 proto files (`polo.proto`, `remotemessage.proto`).

fn main() {
    prost_build::Config::new()
        .compile_protos(
            &["proto/polo.proto", "proto/remotemessage.proto"],
            &["proto"],
        )
        .expect("failed to compile protos");
}
