# State matrix

What exists, what is deferred, and how it is tested. Measured on 2026-10-19 with
`cargo test --workspace`; the numbers are the ones in the output, not estimates.

## Crates

| Crate | Status | Tests | What it does |
|---|---|---|---|
| `tv-rs` (facade) | done | 7 + 1 doc | `discover()`, `identify_vendor()`, `TvClient` dispatch, `TvError` |
| `tv-rs-core` | **new (Phase 1a)** | 36 with `mock-backend conformance quirks`, 20 default | capability traits, `DeviceHandle`, `CommandId`, confidence-tagged `ObservedState`, routing `TvError`, scriptable mock backend, conformance suite, quirks loader |
| `tv-rs-discovery` | done | 6 + 1 doc | SSDP `M-SEARCH`, response parsing, UPnP descriptor, descriptor fetch |
| `tv-rs-power` | done | 9 + 1 doc | magic packet, `MacAddress` parsing, `poll_power_state` |
| `tv-rs-transport` | done | 4 + 2 doc | `TlsPolicy`, rustls/native-tls connector, budgeted frame reader, `Unsupported` |
| `tv-rs-tizen` | done | 35 + 5 doc | remote channel, DIAL, token store, connection pool, SSDP discovery, app presets |
| `tv-rs-webos` | done | 14 + 1 doc | SSAP session, pairing, `client-key` persistence, volume/mute/launch/power |
| `tv-rs-roku` | done | 9 + 1 doc | ECP device-info, apps, keypress, launch, name→id resolution |
| `tv-rs-media` | done | 17 + 6 + 2 doc | range parsing, `206`/`416` serving, HLS playlists |
| `tv-rs-cli` | done (`publish = false`) | 14 + 1 doc | `discover`, `wake`, `power-off`, `key`, `launch` |
| `tests/hardware_in_the_loop.rs` | opt-in | 7 (`--features hil`) | real devices, skipped when `TV_RS_HIL_*` is unset |
| **Total (default features)** | | **121 + 15 doc** | 136 tests |

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
  `TlsError::PinningUnavailable`. What is *not* done is verifying it against a
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

| Task | Why | Size |
|---|---|---|
| mDNS listener | AirPlay/Cast devices are invisible to `discover()` | small |
| More `SsapUri` variants | input/source switching is the most requested webOS gap | small |
| Real-device verification pass | the only way to close limitation 1 | needs hardware |
