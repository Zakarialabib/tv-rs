# State matrix

What exists, what is deferred, and how it is tested. The numbers below were
measured on **2026-10-24**, each crate in an isolated `cargo test -p <crate>` run;
the workspace total is its own `cargo test --workspace` run. The numbers are the
ones in the output, not estimates. Per-crate counts do **not** sum to the workspace
total — feature unification turns optional features on for the whole graph.
Regression commands: `cargo test --workspace`, and per crate
`cargo test -p <crate> [--features …]` as noted per row.

## Crates

| Crate                                | Status                   | Tests                                                                  | What it does                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| ------------------------------------ | ------------------------ | ---------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `tv-rs` (facade)                     | done                     | 41 (lib 40 + doctest; `--features testkit` adds 3; `--features hil` adds the 7-test HIL suite) | `discover()`, `identify_vendor()`, `TvClient` dispatch, `power_observation()` (value + confidence), `TlsPolicy` re-export, quirk-driven port pinning, capability truth (below)                                                                                                                                                                                                                                                                                        |
| `tv-rs-core`                         | done                     | 26 default / 51 with `mock-backend conformance quirks`                 | capability traits, `DeviceHandle`, `CommandId`, confidence-tagged `ObservedState`, routing `TvError` + `Remedy`, LFD vocabulary + traits (`PanelLockControl`, `OrientationControl`, `PanelHealthProvider`; behind `supporting`: `PanelHoursProvider`, `ScreenCapturer`), scriptable mock backend (`force_power` + settable LFD state), conformance suite (truth table + `Unsupported` routing for all fifteen capabilities), quirks loader and `QuirkAction` decoding |
| `tv-rs-discovery`                    | done                     | 8                                                                      | SSDP `M-SEARCH`, response parsing, UPnP descriptor, descriptor fetch                                                                                                                                                                                                                                                                                                                                                                                                  |
| `tv-rs-power`                        | done                     | 10                                                                     | magic packet, `MacAddress` parsing, `poll_power_state`                                                                                                                                                                                                                                                                                                                                                                                                                |
| `tv-rs-local`                        | done                     | 17 = 15 lib + 2 doc (2026-10-23)                                       | host-side EDID/connector/mode probe with a TV-likelihood score; Windows WMI plus the registry EDID of each active monitor, Linux DRM, macOS declared unsupported; public platform-independent EDID base-block parser (`parse_edid`: PNP id, product code, serial, size in cm, name/serial descriptors, preferred timing) tested against two real EDIDs, and `display_edids()` for the raw bytes (E3b)                                                                        |
| `tv-rs-dlna`                         | done                     | 79                                                                     | DLNA renderer transport: SSDP MediaRenderer discovery, AVTransport SOAP (`SetAVTransportURI`/`Play`/`Pause`/`Stop`/`Seek`), GENA subscribe/renew/unsubscribe with `LastChange` parsing, renewal registry, SSDP advertisement for a caller-hosted MediaServer, DIDL-Lite metadata                                                                                                                                                                                      |
| `tv-rs-transport`                    | done                     | 14                                                                     | `TlsPolicy`, rustls/native-tls connector, budgeted frame reader, `Unsupported`, compiles without the `ws` feature                                                                                                                                                                                                                                                                                                                                                     |
| `tv-rs-tizen`                        | done, **experimental**   | 56                                                                     | remote channel, DIAL, token store, connection pool, SSDP discovery, app presets, `pinned_port`, `TizenRemote` service                                                                                                                                                                                                                                                                                                                                                 |
| `tv-rs-webos`                        | done                     | 40                                                                     | SSAP session, pairing, `client-key` persistence, volume/mute/launch/power/foreground/`open_url`, `pinned_port`                                                                                                                                                                                                                                                                                                                                                        |
| `tv-rs-roku`                         | done                     | 23                                                                     | ECP device-info, apps, keypress (with the 14.1 permission gate), launch, name→id resolution                                                                                                                                                                                                                                                                                                                                                                           |
| `tv-rs-mdc`                          | done, **unpublished**    | 28 = 18 lib + 8 tcp + 2 doc · 31 with `serial supporting` (19 / 9 / 3) | Samsung MDC over TCP 1515 **or RS-232** (`serial`): one frame codec + checksum on both carriers, display-ID addressing incl. broadcast, one socket per wall, power/input/volume/mute/brightness/status, the LFD subset (panel lock `0x5F`, orientation `0xC8`/`0x82`, panel health `0x0D`, panel hours `0x83` behind `supporting`); screen capture is never advertised (no MDC command exists); a NAK returns its command and error code and keeps the socket         |
| `tv-rs-media`                        | done                     | 25 = 17 lib + 6 http + 2 doc                                           | range parsing, `206`/`416` serving, HLS playlists                                                                                                                                                                                                                                                                                                                                                                                                                     |
| `tv-rs-agent`                        | done                     | 51 default / 59 with `http mqtt`                                       | desired-state providers (file / HTTP / MQTT), pure decision function, healing ladder, graduated remediation ladder (dim → restart → reboot), JSONL audit log, `tick()`/`run()`, 24-hour fleet simulation                                                                                                                                                                                                                                                              |
| `tv-rs-cec`                          | done                     | 6 default / 7 with `client`                                            | CEC over a `cec-client` subprocess: power on, standby, active source                                                                                                                                                                                                                                                                                                                                                                                                  |
| `tv-rs-app`                          | done, **unpublished**    | 42 = 40 lib + 2 doc (plus a runnable `examples/package.rs`)            | TV app producer: descriptor validation, Tizen `config.xml` + webOS `appinfo.json` generation, bundle layout, a stored-entry ZIP writer (a `.wgt` **is** a ZIP — no compression dependency, byte-identical output), the generated `tv-bridge.js` (D-pad vocabulary → the payload's `key_event`; vendor back/exit numbers come from the deployment, not a guess), and a typed refusal to fabricate an `.ipk`                                                            |
| `tv-rs-cli`                          | done (`publish = false`) | 17                                                                     | `discover`, `wake`, `power-off`, `key`, `launch`, `status` (power + confidence + capabilities), `ensure`                                                                                                                                                                                                                                                                                                                                                              |
| `tests/hardware_in_the_loop.rs`      | opt-in                   | 7 (`--features hil`)                                                   | real devices, skipped when `TV_RS_HIL_*` is unset                                                                                                                                                                                                                                                                                                                                                                                                                     |
| **Total (`cargo test --workspace`)** |                          | **500 across 38 targets**                                              | 22 test binaries + 16 doctest targets, default features                                                                                                                                                                                                                                                                                                                                                                                                               |

## Capability truth

The one honest table: what each backend advertises today. The source of this
section is code — `crates/tv-rs/src/handle.rs` (the facade's `DeviceHandle`
impl), `crates/mdc/src/handle.rs` (`MdcDevice`) and
`crates/core/src/capability.rs` (the fifteen-variant `Capability` enum). When a
page elsewhere disagrees with this table, the table wins; fix the page.

| Backend            | power | wake | key | app | foreground | input | volume | panel lock | orientation | panel health |
| ------------------ | ----- | ---- | --- | --- | ---------- | ----- | ------ | ---------- | ----------- | ------------ |
| Tizen (`TvClient`) | ✓     | —    | ✓   | ✓   | —          | —     | —      | —          | —           | —            |
| webOS (`TvClient`) | ✓     | —    | ✓   | ✓   | ✓          | —     | ✓      | —          | —           | —            |
| Roku (`TvClient`)  | ✓     | —    | ✓   | ✓   | —          | —     | —      | —          | —           | —            |
| MDC (`MdcDevice`)  | ✓     | —    | —   | —   | —          | ✓     | ✓      | ✓          | ✓           | ✓            |

Why each dash is a fact, not a gap:

- **`wake` is absent on every `TvClient`** because a discovered device carries no
  MAC yet, and a wake without one is a lie. Wake lives in `tv-rs-power`, which
  takes the MAC from the caller.
- **`key` on webOS goes through the pointer socket**, not a key endpoint: SSAP has
  no remote-key message, so a press is a `type:button` frame on the connection
  `getPointerInputSocket` hands back. The facade keeps one of those sockets per TV
  and reuses it across presses (`crates/webos/src/pool.rs`); the button vocabulary
  is mapped in `crates/tv-rs/src/handle.rs::webos_key`. Registration must ask for
  `CONTROL_INPUT_TEXT` and `CONTROL_MOUSE_AND_KEYBOARD` or the TV refuses the path.
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
  picture. Each refusal above is sourced in [`01-playbook.md`](01-playbook.md), not
  assumed: the vendor's own protocol documentation is what says the command is absent,
  and the playbook is where those sources are recorded.

**The rule the table enforces: an absent accessor means the protocol cannot do
it** — knowable before any command is sent, and the same fact as the typed
`Unsupported` error after one. UI gating derives from `CapabilitySet`; never
probe at runtime to find out what a vendor can do.

## Published (crates.io, verified 2026-10-22 against the registry API)

| Crate                                                                                                                                                                                                   | Registry                | Note                                                                                                 |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------- | ---------------------------------------------------------------------------------------------------- |
| `tv-rs-core`, `tv-rs-transport`, `tv-rs-discovery`, `tv-rs-power`, `tv-rs-local`, `tv-rs-dlna`, `tv-rs-roku`, `tv-rs-tizen`, `tv-rs-webos`, `tv-rs` (facade), `tv-rs-media`, `tv-rs-agent`, `tv-rs-cec` | `0.2.0`                 | thirteen crates installable today                                                                    |
| `tv-rs-mdc`, `tv-rs-app`                                                                                                                                                                                | **not on the registry** | merged in this repository; their first publish rides the `0.3.0` release ([plan item 1](06-plan.md)) |
| `tv-rs-cli`, `xtask`                                                                                                                                                                                    | —                       | `publish = false`                                                                                    |

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
  firmware — the evidence and the rules are recorded in a private note that is
  deliberately not part of this public set.
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

## Platform truth — where tv-rs runs and who runs it

The sender-side matrix: the host the library compiles on, what compiles with it,
and which consumer lives there. A platform is listed only if something consumes
tv-rs on it, or a plan item names it.

| Host platform                         | Status                              | TLS story                                           | Consumers                                                                                                                                                                           | Open work                                                                                                                                                         |
| ------------------------------------- | ----------------------------------- | --------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Windows desktop                       | outlined, **primary**               | `rustls` default (no OpenSSL); `native-tls` opt-in  | Simple-Signage (Tauri 2 desktop, the signage workstation), tv-rs CLI, tv-rs-agent                                                                                                   | — (A5/C1 explicitly leave desktop untouched)                                                                                                                      |
| Linux desktop                         | consumed, **primary**               | same                                                | signage nodes, agent deployments; `tv-rs-local` probes DRM connectors (Windows WMI / Linux DRM; **macOS EDID is declared unsupported**, not stubbed)                                | peer-comparison deltas below                                                                                                                                      |
| macOS desktop                         | rustls path works                   | same                                                | none claimed in-repo; `tv-rs-local` macOS = `Unsupported`                                                                                                                           | none planned                                                                                                                                                      |
| Android phone / tablet (Tauri mobile) | planned, **Tier A/C-Gated**         | rustls + TOFU/pinning (system roots are the C1 gap) | companion-app (Tauri 2; `AGENTS.md`: desktop-first, no `tauri android` until Tier A lands), Simple-Signage `crates/mobile` (scripts exist for aarch64 targets, `--features mobile`) | A5 — `Blocked`/`RequestPermission` for `ACCESS_LOCAL_NETWORK`; C1 — zero-datagams-vs-zero-TVs, trust roots, `cfg`-gate `tv-rs-local`; G15 `probe(ip)` for pickers |
| Android TV (a _controlled_ device)    | deferred, **researched 2026-10-23** | n/a (device side)                                   | deferred trust model — see the peer landscape row                                                                                                                                   | one spike before any crate: ATR v2 vs ADB (below)                                                                                                                 |

The important asymmetry Android was hiding: the **phone is a sender**, and its
work is already in the plan as A5/C1. The **TV is a target**, and it was written
off for the wrong reason — ADB's trust model. The peer landscape found a second
door.

## Peer landscape — the same work in other languages

Written on 2026-10-23 from the libraries' own READMEs/releases (search-verified
where the text says so; desktop-protocol trivia such as exact ports is **not**
re-measured per panel and never will be here). The point is not to flatter — it
is to steal the strengths and refuse the weaknesses, as gaps.

| Project                                                                     | Language    | What it gets right                                                                                                                                                                                                                                         | What tv-rs already does better or must refuse                                                                                                                                                                                                                        |
| --------------------------------------------------------------------------- | ----------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `androidtvremote2` (Python, v0.3.2 2026-09)                                 | Python      | **Android TV without developer mode**: the Google TV app's own protocol v2 — mTLS with Polo pairing, protobuf, keycodes, deep links, voice; the TV's name + MAC parse out of the **server** cert on the pairing port (no pairing needed). Search-verified. | Single-vendor library, no capability truth, `asyncio` exceptions over a remedy taxonomy. **This is the spike**: server-cert MAC probe → our G4 wake gap; pre-installed service → no ADB. Brief: crystallized 2026-10-23 (`androidtv-v2/references/atr-v2-brief.md`). |
| `atvremote` (Go) / `androidtv-remote` (JS, louis49)                         | Go / Node   | same family; the JS repo is the origin of the v2 `.proto` definitions both Python and Go copy                                                                                                                                                              | CLI-first, no fleet model; proto files are the stealable artifact (they document the wire shape we would write from)                                                                                                                                                 |
| Samsung consumer-crates: `samsungctl`, `samsung-tv-ws-api` lineage          | Python      | proved the remote channel + pin-pairing UX years ago                                                                                                                                                                                                       | legacy SDK-style code, replaced in-repo by `tv-rs-tizen` with a token store, pools and quirk rows; keep their firmware-version scatter as quirks lessons                                                                                                             |
| `pywebostv` / `lgtv2` / `aiopylgtv`                                         | Python/Node | SSAP pointer input, `ssap://` discovery, `client-key` files                                                                                                                                                                                                | no remedy-typed errors; their pointer-socket facts are now implemented (B1 closed) — `crates/webos/src/pointer.rs` plus the facade's key map, with the wire form pinned by tests                                                                                        |
| Home Assistant integrations (`androidtv`, `samsungtvws`, `cast`, `roku`...) | Python      | the pragmatic "everything, all vendors" reference point; users voted the integrations into existence                                                                                                                                                       | HA's signal quality insulates it behind an event bus; tv-rs keeps the fleet contract: capability sets, `Remedy`, conformance, a simulator — the pieces HA never had                                                                                                  |
| `cec-utils` (C) / `libcec`                                                  | C/C++       | the ground truth for CEC transport and key authority                                                                                                                                                                                                       | `tv-rs-cec` shells to `cec-client` on purpose (no `unsafe`, no C toolchain); native `/dev/cec` stays behind a measurement                                                                                                                                            |
| B2B vendor SDKs (Samsung MDC/SPMD docs, LG InstaView CM SDK)                | C#/etc      | documents MDC 1515/RS-232 and LFD commands                                                                                                                                                                                                                 | already implemented, tested and published as `tv-rs-mdc` — the only crate here whose protocol is _fully documented_                                                                                                                                                  |

The immediate consequence for the plan: Android TV enters as a **spike**, not a
crate — measure the ATR v2 pairing handshake and certificate-MAC extraction on a
real panel (or with the REA MCP against the Google TV remote app's APK), record
it in [`hil-results.md`](hil-results.md), and only then decide the trust-model
framing ADB's "not started" note left blank.

## Next

The live plan owns the open work, with an exit criterion per item:
[`06-plan.md`](06-plan.md). This page describes what exists today; what landed
over time is [`../CHANGELOG.md`](../CHANGELOG.md).
