# Implementation Plan: Google Cast sender (`tv-rs-cast`) [COMPLETED]

> **Code moved (0.8.0).** The implementation this plan describes now lives in
> `tv-rs-drivers` (`crates/drivers/src/cast/`); `crates/cast` is a `publish = false`
> deprecated shim. The plan is kept as history — the paths below are the pre-0.8.0
> ones.

Status: **Landed** (PRs #102, #103)

1. [x] Define JSON payload schemas (in `crates/drivers/src/cast/payload.rs`) for namespace CASTV2_1_0_0 (connection, receiver, media, volume, media_status, etc).
2. [x] Implement TLS + EKE2 frame transport (in `crates/drivers/src/cast/transport.rs`) handling Length-Prefixed protobufs and Cast Message namespaces.
3. [x] Implement `CastClient` (in `crates/drivers/src/cast/client.rs`) with API mirroring other vendor adapters (connect, status, launch, load_media, transport controls play/pause/seek, media_status query, set_volume, stop).
4. [x] Implement DIAL HTTP fallback (in `crates/drivers/src/cast/dial.rs`).
5. [x] Add unit tests for JSON, protobuf encode/decode round-trip, and EKE framing using test vectors (mock transport).
6. [x] Update workspace root members in Cargo.toml to include `"crates/cast"`.
7. [x] Update `crates/tv-rs` facade:
    - `tv-rs-cast` dependency routed through `TvClient` facade.
    - Expose CastDevice adapter, PowerControl, KeyInput, and AppLaunch.
8. [x] Complete Pre Commit Steps.
9. [x] Submit the change.
