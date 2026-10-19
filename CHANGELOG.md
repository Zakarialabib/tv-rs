# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **`tv-rs-core`: the device model.** Capability traits (`PowerControl`,
  `KeyInput`, `AppLaunch`, `InputSelect`, `VolumeControl`), the erased
  `DeviceHandle`, `CommandId` with its idempotency contract, confidence-tagged
  `ObservedField`/`ObservedState`, the six-variant routing `TvError`, and a
  scriptable mock backend (`mock-backend`) that reproduces pre-send failures,
  lost acks, malformed replies, mid-session auth expiry and mid-command power-off.
  The crate has no tokio in its dependency graph in any configuration; CI enforces
  that.
- **The facade speaks the device model.** `TvClient` implements `DeviceHandle`
  and the five capability traits, with per-vendor capability sets that match what
  each backend actually does, and backend errors converted into the routing
  taxonomy (`Unsupported`, `Auth(PairingRequired)`, `Transport(Timeout)`, …).
- **Certificate pinning for self-signed TV certificates.** `tv-rs-transport`
  gains `CertFingerprint` (SHA-256, hex round-trip), `TlsPolicy::Pinned` and
  `TlsPolicy::TrustOnFirstUse` with a `CertSink`. The default policy stays
  `Strict`; a pinned channel verifies the exact certificate a TV presented,
  which is what makes Samsung 8002 / LG 3001 usable without
  `TlsPolicy::AcceptInvalid`. Rustls only — a `native-tls` build returns
  `TlsError::PinningUnavailable`.

- **Power state across the facade.** `TvClient::power_state()` answers "is this TV
  on?" without reaching for a vendor crate: Roku is queried through
  `/query/device-info` (`RokuPowerMode`, with `Headless`/`Ready`/`DisplayOff`
  reported as `Standby`, not `Off`), Tizen and webOS fall back to the shared TCP
  probe against their control port.

### Changed

- **Workspace reorganization.** All packages are flattened under `crates/` (one
  home per package), the facade moved from the repository root to
  `crates/tv-rs`, the CLI was promoted from `utils/cli` to `crates/cli`, and the
  root manifest is now a virtual workspace. Release tooling was ported from a
  shell script to `cargo xtask publish`.
- `missing_docs` is enforced once, in `[workspace.lints.rust]`, instead of in
  every crate root.

### Removed

- `roku`'s no-op `RokuAuthProvider` marker trait: Roku ECP is unauthenticated, and
  a marker that always reports "no credentials" was noise.

### Fixed

- The facade's `keywords` contained path-like entries with slashes, which
  crates.io rejects.
- The facade `description` advertised mDNS discovery, which is not implemented;
  mDNS is now described as planned.

No public API changes.

## [0.1.0] - 2026-10-19

### Added

- **Facade (`tv-rs`)** — `discover()` / `discover_with()`, a documented
  `identify_vendor()` heuristic over SSDP and UPnP fields, `DiscoveredTv`, and
  a `TvClient` that dispatches power and app-launch calls across Samsung, LG
  and Roku with typed `as_tizen` / `as_webos` / `as_roku` accessors.
- **`tv-rs-discovery`** — SSDP `M-SEARCH` over UDP 1900 with interface/bind
  overrides, case-insensitive response parsing with USN/LOCATION dedup, and
  UPnP device-descriptor parsing.
- **`tv-rs-power`** — Wake-on-LAN magic packets (UDP 9) and a TCP power-state
  probe distinguishing On / Standby / Off / Unknown.
- **`tv-rs-tizen`** — remote WebSocket channel on 8001/8002 with the
  `ms.channel.connect` handshake and token capture, remote-key frames, and the
  DIAL launcher on 8080.
- **`tv-rs-webos`** — SSAP JSON-RPC over WebSocket on 3000/3001 with pairing,
  `client-key` persistence, and launch/volume/mute/power/foreground commands.
- **`tv-rs-roku`** — unauthenticated ECP REST on 8060: device-info, apps,
  keypress and launch, with pure XML parsers.
- **`tv-rs-transport`** — shared TLS policy, native-tls connector and budgeted
  WebSocket frame reader.
- **`tv-rs-media`** — HTTP `206 Partial Content` serving (open-ended, explicit
  and suffix ranges, `416` handling) plus HLS `VOD` manifest generation.
- **`tv-rs-cli`** — `discover`, `wake`, `power-off`, `key` and `launch`
  subcommands (`publish = false`).
- **Hardware-in-the-loop harness** behind the `hil` feature, skipped unless
  `TV_RS_HIL_*` host variables are set.
- MIT license, CI (rustfmt, clippy, test on Linux+Windows, rustdoc) and an
  ordered multi-crate release script.

### Known limitations

- Vendor authentication that depends on undocumented behaviour (self-signed TLS
  pinning, enterprise token derivation, encrypted framing) is unimplemented. It
  surfaces as typed `Unsupported` errors rather than panics or a disabled
  certificate check.
- mDNS/Bonjour discovery and Android TV ADB are not implemented.
- No crate has been verified against physical hardware in CI; use the `hil`
  harness.
