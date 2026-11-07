# Implementation Plan: Google Cast sender (`tv-rs-cast`) [COMPLETED]

Status: **Landed** (PRs #102, #103)

1. [x] Define JSON payload schemas (in `crates/cast/src/payload.rs`) for namespace CASTV2_1_0_0 (connection, receiver, media, volume, media_status, etc).
2. [x] Implement TLS + EKE2 frame transport (in `crates/cast/src/transport.rs`) handling Length-Prefixed protobufs and Cast Message namespaces.
3. [x] Implement `CastClient` (in `crates/cast/src/client.rs`) with API mirroring other vendor adapters (connect, status, launch, load_media, transport controls play/pause/seek, media_status query, set_volume, stop).
4. [x] Implement DIAL HTTP fallback (in `crates/cast/src/dial.rs`).
5. [x] Add unit tests for JSON, protobuf encode/decode round-trip, and EKE framing using test vectors (mock transport).
6. [x] Update workspace root members in Cargo.toml to include `"crates/cast"`.
7. [x] Update `crates/tv-rs` facade:
    - `tv-rs-cast` dependency routed through `TvClient` facade.
    - Expose CastDevice adapter, PowerControl, KeyInput, and AppLaunch.
8. [x] Complete Pre Commit Steps.
9. [x] Submit the change.
