# State matrix

What exists, what is deferred, and how it is tested. Measured on 2026-10-20 with
`cargo test --workspace`; the numbers are the ones in the output, not estimates.
Per-crate counts come from separate `-p` runs, so they do not sum to the workspace
total — feature unification turns optional features on for the whole graph.

## Crates

| Crate | Status | Tests | What it does |
|---|---|---|---|
| `tv-rs` (facade) | done | 22 | `discover()`, `identify_vendor()`, `TvClient` dispatch, `power_observation()` (value + confidence), `TlsPolicy` re-export, quirk-driven port pinning |
| `tv-rs-core` | done (Phase 1a) | 21 default / 42 with `mock-backend conformance quirks` | capability traits, `DeviceHandle`, `CommandId`, confidence-tagged `ObservedState`, routing `TvError`, scriptable mock backend (now with `force_power`), conformance suite, quirks loader **and** `QuirkAction` decoding |
| `tv-rs-discovery` | done | 7 | SSDP `M-SEARCH`, response parsing, UPnP descriptor, descriptor fetch |
| `tv-rs-power` | done | 10 | magic packet, `MacAddress` parsing, `poll_power_state` |
| `tv-rs-transport` | done | 14 | `TlsPolicy`, rustls/native-tls connector, budgeted frame reader, `Unsupported`, compiles without the `ws` feature |
| `tv-rs-tizen` | done, **experimental** | 43 | remote channel, DIAL, token store, connection pool, SSDP discovery, app presets, `pinned_port` |
| `tv-rs-webos` | done | 18 | SSAP session, pairing, `client-key` persistence, volume/mute/launch/power, `pinned_port` |
| `tv-rs-roku` | done | 16 | ECP device-info, apps, keypress (with the 14.1 permission gate), launch, name→id resolution |
| `tv-rs-mdc` | **new (U4)** | 17 + 5 tcp + 2 doc | Samsung MDC over TCP 1515: frame codec + checksum, display-ID addressing incl. broadcast, one socket per wall, power/input/volume/mute/brightness/status; a NAK returns its command and error code |
| `tv-rs-media` | done | 25 | range parsing, `206`/`416` serving, HLS playlists |
| `tv-rs-agent` | done (Phase 4) | 51 default / 59 with `http mqtt` | desired-state providers (file / HTTP / MQTT), pure decision function, healing ladder, graduated remediation ladder (dim → restart → reboot), JSONL audit log, `tick()`/`run()`, 24-hour fleet simulation |
| `tv-rs-cec` | done (Phase 5a) | 6 default / 7 with `client` | CEC over a `cec-client` subprocess: power on, standby, active source |
| `tv-rs-cli` | done (`publish = false`) | 17 | `discover`, `wake`, `power-off`, `key`, `launch`, `status` (power + confidence + capabilities), `ensure` |
| `tests/hardware_in_the_loop.rs` | opt-in | 7 (`--features hil`) | real devices, skipped when `TV_RS_HIL_*` is unset |
| **Total (`cargo test --workspace`)** | | **237 across 29 binaries** | includes doctests and the three `-p tv-rs-media` suites |

## Published (crates.io, 2026-10-20)

| Crate | Version | Note |
|---|---|---|
| `tv-rs-core`, `tv-rs-transport`, `tv-rs-discovery`, `tv-rs-power` | `0.2.0` | |
| `tv-rs-local`, `tv-rs-dlna` | `0.2.0` | first published today (were `0.1.0` for a few hours) |
| `tv-rs-roku`, `tv-rs-tizen`, `tv-rs-webos` | `0.2.0` | |
| `tv-rs` (facade), `tv-rs-media`, `tv-rs-agent`, `tv-rs-cec` | `0.2.0` | |
| `tv-rs-mdc` | **unpublished** | merged; publishes after `tv-rs-core` gains the telemetry/brightness/reboot APIs on the registry |
| `tv-rs-cli`, `xtask` | — | `publish = false` |

Thirteen crates are installable; the fourteenth exists in git and will publish with the
next core release.

`cargo check --workspace --all-targets`, `cargo test --workspace`,
`cargo clippy --workspace --all-targets -- -D warnings` and
`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` all pass locally and
in CI (Linux and Windows).

## Per-crate detail

**Facade.** `discover()` / `discover_with(&SsdpScanner)`; `identify_vendor()`
over `SERVER`/`ST`/`USN` plus descriptor fields; `TvClient` with `power_off()`,
`power_state()`, `launch_app()` and typed accessors. `power_state()` asks Roku
outright and falls back to the TCP probe for Tizen/webOS. `TvClient` also
implements the `tv-rs-core` `DeviceHandle` traits, advertising per vendor only
what works: Tizen has power/key/app, webOS power/app/foreground/volume, Roku
power/key/app — and none advertise wake (no MAC) or input selection (not
implemented). Covers only operations every vendor has;
anything else goes through the accessor.

**Discovery.** Interface/bind overrides, timeout, USN/LOCATION dedup, descriptor
fetch. mDNS/Bonjour and ARP are not implemented — the enum names them, the code
does not.

**Power.** 102-byte magic packet, broadcast override, `MacAddress` accepting
colon/dash/bare-hex, TCP probe → `On`/`Standby`/`Off`/`Unknown`. WoWLAN wake
reasons are not modelled.

**Transport.** Two mutually exclusive TLS backends (`rustls` default,
`native-tls` opt-in) enforced by `compile_error!`; certificate pinning
(`Pinned` / `TrustOnFirstUse`); the bounded frame reader both vendor crates use;
and `should_try_next_port()`, the rule that lets a client try the other port
generation when — and only when — nothing answered.

**Tizen.** 8001/8002 channel with token capture; DIAL launch; 22-key enum;
`SamsungTokenStore` (quarantine + no-clobber upsert); `TizenConnectionPool` (one
channel per TV, one reopen on a dead socket); `discover_samsung()` via the
Samsung-specific search target; `resolve_app_id()` presets.

**webOS.** Registration + pairing, `client-key` save/load, `SsapUri` enum,
volume/mute/launch/power/foreground/**open_url**, port ladder 3001 → 3000 with the
negotiated port cached. Input switching is additive work.

**Roku.** Device-info/apps/keypress/launch with pure XML parsers; `query_app_id`
resolves a name to an id; `<power-mode>` is parsed into `RokuPowerMode`
(`PowerOn`/`Headless`/`Ready`/`DisplayOff`/`Unknown`), with the awake states mapped
to `Standby` rather than `Off` so a working device never reads as dead. `roku:ecp` SSDP discovery is covered by the facade's
scan, not a dedicated path.

**Media.** Pure `parse_range` (open-ended, explicit, suffix, clamping, `416`);
axum handler streaming the slice; HLS playlist from a directory of segments; path
traversal rejected before any file open.

## Deferred

- **Certificate pinning** for Samsung 8002 / LG 3001 — implemented as
  `TlsPolicy::TrustOnFirstUse` (record) and `TlsPolicy::Pinned` (verify) in
  `tv-rs-transport`, rustls only; a `native-tls` build returns
  `TlsError::PinningUnavailable`. What is _not_ done is verifying it against a
  real TV's certificate.
- **Enterprise token derivation** and **encrypted framing** on newer Samsung
  firmware.
- **mDNS/Bonjour** discovery, **ARP** resolution, **WoWLAN** reasons.
- **Android TV ADB**, **Google Cast**, **AirPlay** — separate trust models,
  separate crates, not started.
- **HLS segmentation** — the server serves existing segments; it does not cut
  them.

Every deferred call returns a typed `Unsupported` error. Nothing panics and
nothing pretends to work.

## Quirks: applied, not just stored

`quirks/default.toml` is loaded by `tv-rs-core` and **acted on** by the facade: with
the `quirks` feature, `TvClient::from_discovered` decodes the matching row's `action`
(`QuirkAction`) and pins the port it names, so the generation a firmware dropped is
never probed. Without the feature the ladder still negotiates — it just pays one
refused connection to learn what the table already knew.

Rows are keyed on what discovery can actually supply. The UPnP descriptor gives
`modelName` but no firmware version, so each vendor has at least one model-scoped row
(`QN9*`, not only `T-KTM*`); a firmware-scoped row is more specific and sits above it.

## Known limits worth knowing before shipping

1. **No hardware in CI.** The `hil` suite is the only thing that touches a real
   TV, and it is opt-in. An SSDP `M-SEARCH` probe from the development host
   (2026-10-19) returned 0 non-loopback responders, so the hardware pass is
   still unrun — no claim below is device-verified. Every published claim about a device behaviour comes from
   the protocol documents plus the pure-function tests.
2. **Two TLS backends, one selected.** A consumer that needs `native-tls` must
   enable it explicitly; enabling both is a compile error, by design.
3. **Vendor quirks are per model year.** The playbook describes the mainstream
   behaviour; a 2019 panel and a 2024 panel can disagree with it.
4. **The media crate is not a DLNA control point.** It serves bytes and
   playlists; UPnP `AVTransport`/`ContentDirectory` are out of scope.

## Next

Six items, and the live plan owns them: [`06-plan.md`](06-plan.md). The two that are
*not* there, on purpose:

| Task | Why it is not on the plan |
|---|---|
| mDNS listener | nothing consumes it yet; AirPlay/Cast would need their own crates first |
| More `SsapUri` variants | input switching has no consumer demanding it, and every variant is a protocol claim that needs hardware to verify |

Everything else — telemetry, MDC serial, Sony Bravia, native CEC, DDC/CI, the
real-device verification pass, the next release — is sequenced there with exit
criteria.
