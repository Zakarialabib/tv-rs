# tv-rs — State Matrix (Done / Left / Next)

**Verified on disk (2026-10-18).** Root `Cargo.toml` is a publishable facade
(`[package] tv-rs`) plus a `[workspace]` over eight root-level member directories
(`cli/`, `discovery/`, `media/`, `power/`, `roku/`, `tizen/`, `transport/`,
`webos/`), sharing `[workspace.package]` and `[workspace.dependencies]`. Five
protocol crates are implemented to Phase-1 depth with real types, real transports
and passing tests, and draw their shared vocabulary from `transport/`.

| Target | Unit / integration tests | Doctests |
|---|---|---|
| `tv-rs` (facade) | 7 | 1 |
| `tv-rs-cli` | 14 | 1 |
| `tv-rs-discovery` | 6 | 1 |
| `tv-rs-media` | 17 unit + 6 socket-level | 2 |
| `tv-rs-power` | 9 | 1 |
| `tv-rs-roku` | 9 | 1 |
| `tv-rs-tizen` | 12 | 1 |
| `tv-rs-transport` | 3 | 2 |
| `tv-rs-webos` | 14 | 1 |
| `hardware_in_the_loop` (`--features hil`) | 7 | — |
| **Total (default features)** | **97** | **11** |

`tests/hardware_in_the_loop.rs` compiles to 0 tests unless `--features hil`.

`cargo check --workspace --all-targets` and `cargo test --workspace` both pass (108 tests
with doctests; the `hil` suite compiles to zero tests unless `--features hil` is set).
`cargo clippy --workspace --all-targets -- -D warnings` passes. (An earlier
revision of this document recorded that clippy could not launch on the
development host; the `clippy` rustup component was broken and has since been
reinstalled, so the real lint gate now runs locally as well as in CI.)

---

## Component Matrix

| Area | Playbook § | Crate / Module | What's Done | What's Left | Priority / Next Action |
|---|---|---|---|---|---|
| Facade | — | `tv-rs` (`src/lib.rs`, `client.rs`, `types.rs`, `error.rs`) | Re-exports all five protocol crates; `identify_vendor()` heuristic; `discover()` / `discover_with()`; `TvClient` aggregate with `power_off()` / `launch_app()` dispatch and typed `as_tizen` / `as_webos` / `as_roku` accessors; `TvError` aggregating all five crate errors via `#[from]` | Deeper cross-vendor conveniences (input switching, volume) once a uniform model exists; `tv-rs` <-> `ezviz-openapi-rs` bridge (see `SPEC-011`) | Add the ezviz bridge as a **separate** crate so `tv-rs` keeps zero camera dependencies |
| Transport | — | `transport/` | The workspace's shared vocabulary: `TlsPolicy` and the connector (the single home of the `danger_accept_invalid_*` calls), `WsStream` / `ReadOutcome` / budgeted `read_until` frame reader, and the `Unsupported { phase, reason }` struct that all five protocol error enums wrap as `#[error(transparent)]`. TLS + WebSocket code sits behind the default-off `ws` feature | Nothing pending — the four real duplications it was created to remove are gone | Keep it vocabulary-thin: `discovery`/`power`/`roku` take `default-features = false`; a third `read_until` consumer belongs in a protocol crate, not here |
| Discovery | §1 | `discovery/` | `SsdpScanner` (UDP 1900 multicast, `join_multicast_v4`, timeout, interface/bind overrides), `build_msearch`, `parse_msearch_response` (case-insensitive headers, `USN`/`LOCATION` dedup), `parse_device_descriptor` (roxmltree), `discover_tvs` (discover + descriptor fetch), `SsdpDiscovery` trait, `DiscoveryMethod` enum | mDNS/Bonjour on 5353, ARP MAC↔IP resolution | P2 — mDNS listener (`_googlecast._tcp`, `_airplay._tcp`) |
| Power | §2 | `power/` | `MacAddress` parsing, `build_magic_packet` (102-byte, UDP 9 broadcast), `WolSender`, `poll_power_state`, `PowerControl` trait | WoWLAN wake reasons; vendor-specific standby detection tuning | P2 — wire `poll_power_state` into facade `TvClient` as a liveness probe |
| Samsung Tizen | §3 | `tizen/` | Unencrypted 8001 / TLS 8002 WebSocket remote channel, DIAL launcher on 8080, token capture/extract, `TizenRemoteKey` (22 keys), `TizenSession::{send_key, send_raw, recv_json}`, `TlsPolicy`, `WebSocketHandshake` / `DialLauncher` traits, `TizenAuthProvider` | §10 Gap A TLS certificate-pinning bypass, §10 Gap B enterprise token math, §10 Gap D AES-GCM framing | Blocked on REA; the gaps surface as `TizenError::Unsupported`, never a panic |
| LG webOS | §4 | `webos/` | SSAP JSON-RPC over 3000/3001, pairing handshake + `client-key` persistence (`save_client_key`/`load_client_key`), `WebosSession::{request, set_volume, get_volume, set_mute, launch_app, power_off, foreground_app}`, `SsapUri` enum, `SsapController` trait, `WebosAuthProvider` | §10 Gap A (port 3001 self-signed pinning), richer `SsapUri` coverage | P2 — add input/source switching URIs |
| Roku ECP | §5 | `roku/` | Unauthenticated ECP REST on 8060: `query_device_info`, `query_apps`, `query_app_id`, `send_keypress` (`RokuKey`, 22 keys), `launch_app`, `power_off` / `power_on`; XML parsers as pure functions; `EcpRestClient` trait, `RokuAuthProvider` | `roku:ecp` SSDP-based discovery path; `query/apps` icon URLs | P2 — parse the ECP device-info `<power-mode>` for true power state |
| Android TV ADB | §6 | Not a crate | Explicitly excluded | ADB TCP/IP (5555), `am start`, `input keyevent` | P3 — child crate `tv-rs-adb` |
| DLNA / HLS | §7-8 | `media/` | HTTP 206 `Range` serving (`range.rs`, `server.rs`), HLS `VOD` playlist generation (`hls.rs`), traversal-safe path join, socket-level tests | `AVTransport` / `RenderingControl` SOAP control (DLNA *push*), on-the-fly segmentation | P3 — add DLNA SOAP against the existing media HTTP plane |
| Google Cast / AirPlay | §9 | Not a crate | Explicitly excluded | Cast protobuf (REA Gap E) via `prost-build`, AirPlay RAOP + FairPlay | P3 — child crate `tv-rs-cast` after REA protobuf output exists |

---

## Phase 2 & 3 Roadmap (Post-Skeleton)

Phase 1 built a structurally sound workspace with no consumers. The standard's
governing principle — *every abstraction must be earned by an actual consumer* —
is what these goals discharge. Phase 1's protocol crates only become an
*abstraction* once something calls them.

| Goal | Description | Standard Constraint | Status |
| :--- | :--- | :--- | :--- |
| **The Consumer** | Build `tv-rs-cli` to earn the facade abstraction. | No `unwrap()`, graceful `TvError` handling. | ✅ Done — `cli/` (765 LOC), 14 tests, `discover|wake|power-off|key|launch`, no `unwrap` outside `#[cfg(test)]`, exit codes from `TvError` |
| **Hardware Harness** | `hardware_in_the_loop.rs` gated by `cfg(feature)`. | Must not run in default `cargo test`. | ✅ Done — `tests/hardware_in_the_loop.rs` + `[features] hil`; default `cargo test` runs 0 of its tests, `--features hil` runs 7 (skip without `TV_RS_HIL_*`) |
| **REA Integration** | Extract Samsung TLS pinning & LG SSAP crypto via JADX. | Replace `Unsupported` errors with real math. | ⏳ Blocked — needs APK artifacts (SmartThings/ThinQ/Roku); tooling = `mcp__rea__*` (see `docs/03-external-refs.md`) |
| **Media Server** | `tv-rs-media` (Axum + 206 Partial Content + HLS). | Must not pollute protocol crate dependency graphs. | ✅ Done — `media/` (`range.rs`, `hls.rs`, `server.rs`), 17 unit + 6 socket-level tests + 2 doctests; only `media` pulls `axum`/`tokio-util` |
| **Release Script** | Ordered publish script for 8 crates. | Must handle `path` stripping and version bumps. | ✅ Done — `scripts/release.sh` orders transport→discovery→power→roku→tizen→webos→tv-rs→media, bumps versions, dry-runs, dirty-tree guard |

Dependency order between the goals: **The Consumer** and **Hardware Harness**
are independent and come first, because both are pure consumers of the existing
facade — they need no new protocol work and they are what proves the facade is
usable. **Release Script** is independent of all of them but gates publication.
**REA Integration** repeats per gap and only replaces bodies behind existing
typed errors. **Media Server** comes last: it is a new data plane whose API
should be designed against a facade that has already been exercised by a real
consumer, not against an unproven one.

---

## Publish Readiness

`cargo publish` is planned, which constrains the design in three ways:

1. **Publishing is not atomic.** crates.io has yank, not delete, so a partial
   failure burns the version. Publish ordering must be
   `transport` → `discovery` → `power` → `roku` → `tizen` → `webos` → `tv-rs`
   → `media` (each facade dependency carries `version` beside `path`, because
   Cargo strips `path` at publish time). `transport` leads the order because
   every other workspace crate depends on it for the `Unsupported` vocabulary.
2. **No panics in library code.** Every blocked capability returns a typed
   error (`TizenError::Unsupported`, `TvError::UnsupportedVendor`,
   `TvError::UnsupportedOperation`) so a published crate can never abort a
   consumer's process.
3. **`#![deny(missing_docs)]` on every crate root**, with a compiling doctest
   in each — verified by `cargo test --workspace`.

## Remaining Gaps

1. **Release tooling cannot publish and is not yet written.** `.github/workflows/release.yml`
   runs a single root-level `cargo publish`, which would fail: the facade's
   dependencies are not on crates.io, and Cargo strips `path` at publish time.
   Eight crates must go out in dependency order with no stable
   `cargo publish --workspace`. Needs a script or `cargo-release` /
   `publish-crates` config. See `ADR-001-crate-publication.md`.
2. **REA-gated auth (§10 Gaps A–E) is unimplemented** — deliberately, and it
   fails closed rather than open.
3. **mDNS / Bonjour and ARP are not implemented.** `DiscoveryMethod` names them,
   but only SSDP runs. This means AirPlay and Google Cast devices are invisible to
   `discover()`.
4. **Two TLS stacks compile into the dependency graph** (`native-tls` via
   `tokio-tungstenite` for tizen/webos, `rustls` via `reqwest` for
   discovery/roku). This is the concrete cost of the crate split; a single crate
   with feature flags would buy the same isolation with atomic publishing.
5. **Hardware verification is harnessed, not yet exercised.** `tests/hardware_in_the_loop.rs` exists (`--features hil`, `TV_RS_HIL_*` env) but no TV has been driven through it in this workspace yet.
   mocks; no crate has been exercised against an actual TV.
