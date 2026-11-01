# Implementation Plan: Google Cast sender (`tv-rs-cast`)

1. Define JSON payload schemas (in `crates/cast/src/payload.rs`) for namespace CASTV2_1_0_0 (connection, receiver, media, volume, etc).
2. Implement TLS + EKE2 frame transport (in `crates/cast/src/transport.rs`) handling Length-Prefixed protobufs and Cast Message namespaces.
3. Implement `CastClient` (in `crates/cast/src/client.rs`) with API mirroring other vendor adapters (connect, status, launch, load_media, transport, set_volume, stop).
4. Implement DIAL HTTP fallback (in `crates/cast/src/dial.rs`).
5. Add unit tests for JSON, protobuf encode/decode round-trip, and EKE framing using test vectors (mock transport).
6. Update workspace root members in Cargo.toml to include `"crates/cast"`.
7. Update `crates/tv-rs` facade:
    - `tv-rs-cast` dependency behind `cast` feature.
    - Expose CastDevice adapter.
8. Complete Pre Commit Steps.
9. Submit the change.
