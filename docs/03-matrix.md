# State matrix

What exists, what is deferred, and how it is tested. The numbers below were
measured on **2026-10-22**, each crate in an isolated `cargo test -p <crate>` run;
the workspace total is its own `cargo test --workspace` run. The numbers are the
ones in the output, not estimates. Per-crate counts do **not** sum to the workspace
total — feature unification turns optional features on for the whole graph.
Regression commands: `cargo test --workspace`, and per crate
`cargo test -p <crate> [--features …]` as noted per row.

## Crates

| Crate                                | Status                   | Tests                                                                  | What it does                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| ------------------------------------ | ------------------------ | ---------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `tv-rs` (facade)                     | done                     | 22 (lib 21 + doctest; `--features hil` adds the 7-test HIL suite)      | `discover()`, `identify_vendor()`, `TvClient` dispatch, `power_observation()` (value + confidence), `TlsPolicy` re-export, quirk-driven port pinning, capability truth (below)                                                                                                                                                                                                                                                                                        |
| `tv-rs-core`                         | done                     | 23 default / 48 with `mock-backend conformance quirks`                 | capability traits, `DeviceHandle`, `CommandId`, confidence-tagged `ObservedState`, routing `TvError` + `Remedy`, LFD vocabulary + traits (`PanelLockControl`, `OrientationControl`, `PanelHealthProvider`; behind `supporting`: `PanelHoursProvider`, `ScreenCapturer`), scriptable mock backend (`force_power` + settable LFD state), conformance suite (truth table + `Unsupported` routing for all fifteen capabilities), quirks loader and `QuirkAction` decoding |
| `tv-rs-discovery`                    | done                     | 7                                                                      | SSDP `M-SEARCH`, response parsing, UPnP descriptor, descriptor fetch                                                                                                                                                                                                                                                                                                                                                                                                  |
| `tv-rs-power`                        | done                     | 10                                                                     | magic packet, `MacAddress` parsing, `poll_power_state`                                                                                                                                                                                                                                                                                                                                                                                                                |
| `tv-rs-local`                        | done                     | 9                                                                      | host-side EDID/connector/mode probe with a TV-likelihood score; Windows WMI, Linux DRM, macOS declared unsupported                                                                                                                                                                                                                                                                                                                                                    |
| `tv-rs-dlna`                         | done                     | 79                                                                     | DLNA renderer transport: SSDP MediaRenderer discovery, AVTransport SOAP (`SetAVTransportURI`/`Play`/`Pause`/`Stop`/`Seek`), GENA subscribe/renew/unsubscribe with `LastChange` parsing, renewal registry, SSDP advertisement for a caller-hosted MediaServer, DIDL-Lite metadata                                                                                                                                                                                      |
| `tv-rs-transport`                    | done                     | 14                                                                     | `TlsPolicy`, rustls/native-tls connector, budgeted frame reader, `Unsupported`, compiles without the `ws` feature                                                                                                                                                                                                                                                                                                                                                     |
| `tv-rs-tizen`                        | done, **experimental**   | 56                                                                     | remote channel, DIAL, token store, connection pool, SSDP discovery, app presets, `pinned_port`, `TizenRemote` service                                                                                                                                                                                                                                                                                                                                                 |
| `tv-rs-webos`                        | done                     | 18                                                                     | SSAP session, pairing, `client-key` persistence, volume/mute/launch/power/foreground/`open_url`, `pinned_port`                                                                                                                                                                                                                                                                                                                                                        |
| `tv-rs-roku`                         | done                     | 16                                                                     | ECP device-info, apps, keypress (with the 14.1 permission gate), launch, name→id resolution                                                                                                                                                                                                                                                                                                                                                                           |
| `tv-rs-mdc`                          | done, **unpublished**    | 27 = 18 lib + 7 tcp + 2 doc · 30 with `serial supporting` (19 / 8 / 3) | Samsung MDC over TCP 1515 **or RS-232** (`serial`): one frame codec + checksum on both carriers, display-ID addressing incl. broadcast, one socket per wall, power/input/volume/mute/brightness/status, the LFD subset (panel lock `0x5F`, orientation `0xC8`/`0x82`, panel health `0x0D`, panel hours `0x83` behind `supporting`); screen capture is never advertised (no MDC command exists); a NAK returns its command and error code and keeps the socket         |
| `tv-rs-media`                        | done                     | 25 = 17 lib + 6 http + 2 doc                                           | range parsing, `206`/`416` serving, HLS playlists                                                                                                                                                                                                                                                                                                                                                                                                                     |
| `tv-rs-agent`                        | done                     | 51 default / 59 with `http mqtt`                                       | desired-state providers (file / HTTP / MQTT), pure decision function, healing ladder, graduated remediation ladder (dim → restart → reboot), JSONL audit log, `tick()`/`run()`, 24-hour fleet simulation                                                                                                                                                                                                                                                              |
| `tv-rs-cec`                          | done                     | 6 default / 7 with `client`                                            | CEC over a `cec-client` subprocess: power on, standby, active source                                                                                                                                                                                                                                                                                                                                                                                                  |
| `tv-rs-cli`                          | done (`publish = false`) | 17                                                                     | `discover`, `wake`, `power-off`, `key`, `launch`, `status` (power + confidence + capabilities), `ensure`                                                                                                                                                                                                                                                                                                                                                              |
| `tests/hardware_in_the_loop.rs`      | opt-in                   | 7 (`--features hil`)                                                   | real devices, skipped when `TV_RS_HIL_*` is unset                                                                                                                                                                                                                                                                                                                                                                                                                     |
| **Total (`cargo test --workspace`)** |                          | **397 across 36 targets**                                              | 21 test binaries + 15 doctest targets, default features                                                                                                                                                                                                                                                                                                                                                                                                               |

## Capability truth

The one honest table: what each backend advertises today. The source of this
section is code — `crates/tv-rs/src/handle.rs` (the facade's `DeviceHandle`
impl), `crates/mdc/src/handle.rs` (`MdcDevice`) and
`crates/core/src/capability.rs` (the fifteen-variant `Capability` enum). When a
page elsewhere disagrees with this table, the table wins; fix the page.

| Backend            | power | wake | key | app | foreground | input | volume | panel lock | orientation | panel health |
| ------------------ | ----- | ---- | --- | --- | ---------- | ----- | ------ | ---------- | ----------- | ------------ |
| Tizen (`TvClient`) | ✓     | —    | ✓   | ✓   | —          | —     | —      | —          | —           | —            |
| webOS (`TvClient`) | ✓     | —    | —   | ✓   | ✓          | —     | ✓      | —          | —           | —            |
| Roku (`TvClient`)  | ✓     | —    | ✓   | ✓   | —          | —     | —      | —          | —           | —            |
| MDC (`MdcDevice`)  | ✓     | —    | —   | —   | —          | ✓     | ✓      | ✓          | ✓           | ✓            |

Why each dash is a fact, not a gap:

- **`wake` is absent on every `TvClient`** because a discovered device carries no
  MAC yet, and a wake without one is a lie. Wake lives in `tv-rs-power`, which
  takes the MAC from the caller.
- **`key` is absent on webOS** because SSAP has no remote-key endpoint — it wants
  a pointer-input socket.
- **`volume` is absent on Tizen and Roku** because both expose relative keys, not
  an absolute level, so `set_volume` cannot be honoured.
- **The LFD columns are absent on the consumer `TvClient` rows per protocol**:
  the consumer remote channels (Tizen 8001/8002, SSAP, ECP) have no panel-lock,
  orientation or health command. The MDC row says `Some` because the MDC
  commands (`0x5F`, `0xC8`/`0x82`, `0x0D`) exist. Neither side invents data.
- **`MdcDevice` also speaks what the table's columns do not show**: brightness,
  reboot (the `0x11` power command carries it as a sub-code), a control-channel
  probe;
  panel hours (`0x83`) behind the core's `supporting` feature. It advertises no
  aggregate **`Telemetry`** — MDC has no signal-strength history or uptime — and
  **never** advertises `ScreenCapture`: no MDC command captures the panel's
  picture. See [`06-plan.md`](06-plan.md#1-release-030--blocked-on-a-token)'s
  rejected-list for the sources behind that refusal.

**The rule the table enforces: an absent accessor means the protocol cannot do
it** — knowable before any command is sent, and the same fact as the typed
`Unsupported` error after one. UI gating derives from `CapabilitySet`; never
probe at runtime to find out what a vendor can do.

## Published (crates.io, verified 2026-10-22 against the registry API)

| Crate                                                                                                                                                                                                   | Registry                | Note                                                                                               |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------- | -------------------------------------------------------------------------------------------------- |
| `tv-rs-core`, `tv-rs-transport`, `tv-rs-discovery`, `tv-rs-power`, `tv-rs-local`, `tv-rs-dlna`, `tv-rs-roku`, `tv-rs-tizen`, `tv-rs-webos`, `tv-rs` (facade), `tv-rs-media`, `tv-rs-agent`, `tv-rs-cec` | `0.2.0`                 | thirteen crates installable today                                                                  |
| `tv-rs-mdc`                                                                                                                                                                                             | **not on the registry** | merged in this repository; its first publish rides the `0.3.0` release ([plan item 1](06-plan.md)) |
| `tv-rs-cli`, `xtask`                                                                                                                                                                                    | —                       | `publish = false`                                                                                  |

The workspace is tagged `0.3.0` and blocked on a crates.io token with
`publish-update` scope; until that publishes, `tv-rs = "0.2"` is what the
registry serves. This is the most common rot vector in these docs: every
published-version claim elsewhere must agree with this table.

`cargo check --workspace --all-targets`, `cargo test --workspace`,
`cargo clippy --workspace --all-targets -- -D warnings` and
`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` all pass locally and
in CI (Linux and Windows).

## Deferred

- **Hardware verification of certificate pinning.** `TlsPolicy::TrustOnFirstUse`
  (record) and `TlsPolicy::Pinned` (verify) are implemented, rustls only; a
  `native-tls` build returns `TlsError::PinningUnavailable`. What is not done is
  verifying them against a real TV's certificate — [plan item 5](06-plan.md).
- **Enterprise token derivation** and **encrypted framing** on newer Samsung
  firmware — the evidence and the rules live in
  [`internal/deferred-auth.md`](internal/deferred-auth.md).
- **mDNS/Bonjour** discovery, **ARP** resolution, **WoWLAN** reasons.
- **Android TV ADB**, **Google Cast**, **AirPlay** — separate trust models,
  separate crates, not started.
- **HLS segmentation** — the server serves existing segments; it does not cut
  them.

Every deferred call returns a typed `Unsupported` error. Nothing panics and
nothing pretends to work.

## Quirks: applied, not just stored

`quirks/default.toml` is loaded by `tv-rs-core` and **acted on** by the facade:
with the `quirks` feature, `TvClient::from_discovered` decodes the matching row's
`action` (`QuirkAction`) and pins the port it names, so the generation a firmware
dropped is never probed. Without the feature the ladder still negotiates — it
just pays one refused connection to learn what the table already knew. Rows are
keyed on what discovery can actually supply: the UPnP descriptor gives
`modelName` but no firmware version, so each vendor has at least one
model-scoped row; a firmware-scoped row is more specific and sits above it.
Submitting a row: [`QUIRKS.md`](../QUIRKS.md).

## Known limits worth knowing before shipping

1. **No hardware in CI.** The `hil` suite is the only thing that touches a real
   TV, and it is opt-in. An SSDP `M-SEARCH` probe from the development host
   (2026-10-19) returned 0 non-loopback responders, so the hardware pass is
   still unrun — no device-behaviour claim in this repository is
   device-verified. Every published claim comes from the protocol documents plus
   the pure-function tests; the empty record is
   [`hil-results.md`](hil-results.md).
2. **Two TLS backends, one selected.** A consumer who needs `native-tls` must
   enable it explicitly; enabling both is a compile error, by design.
3. **Vendor quirks are per model year.** The playbook describes the mainstream
   behaviour; a 2019 panel and a 2024 panel can disagree with it.
4. **The media crate is not a DLNA control point.** Serving bytes and playlists
   is `tv-rs-media`; renderer control is `tv-rs-dlna`; UPnP
   `ContentDirectory`/media-library stays application-owned.

## Next

The live plan owns the open work, with an exit criterion per item:
[`06-plan.md`](06-plan.md). This page describes what exists today; what landed
over time is [`../CHANGELOG.md`](../CHANGELOG.md).
