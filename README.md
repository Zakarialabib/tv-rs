# tv-rs

[![CI](https://github.com/Zakarialabib/tv-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/Zakarialabib/tv-rs/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/tv-rs.svg)](https://crates.io/crates/tv-rs)
[![docs.rs](https://docs.rs/tv-rs/badge.svg)](https://docs.rs/tv-rs)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![MSRV 1.89](https://img.shields.io/badge/MSRV-1.89-blue.svg)](Cargo.toml)

Find the televisions on your LAN and drive them from Rust — discovery, power,
remote control, fleet reconciliation and media serving, behind one small facade.

> **Version check.** `0.8.0` is on crates.io for all **nineteen** published crates.
> Every vendor protocol lives in one feature-flagged crate, `tv-rs-drivers`; the ten
> old `tv-rs-<vendor>` names are workspace-only deprecated shims and are **not**
> published. The surface this page describes — `TvClient` (Google Cast and HDMI-CEC
> included), `power_observation()`, the `quirks` feature, `tv-rs-companion`,
> `tv-rs-provision`, `tv-rs-dlna`, `tv-rs-app` and the transport controls — is
> published on [docs.rs](https://docs.rs/tv-rs).

```rust
use tv_rs::discover;

for tv in discover().await? {
    println!("{} ({}) at {}", tv.label(), tv.vendor.as_str(), tv.ip);
    let client = tv.to_client()?;
    println!("  power: {:?}", client.power_state().await?);
}
```

## Why this exists

Every TV vendor speaks a different protocol on the same LAN, and none of the
libraries that wrap them agree on anything: one returns a socket, another a future
of XML, a third panics when the TV is asleep. This workspace puts every protocol
behind the same feature-flagged crate and the same module shape, then adds two layers
on top:

- a **device model** (`tv-rs-core`) that says what a TV can do, what it is believed
  to be doing, and what a failure means — so callers branch on capability and remedy
  instead of on vendor;
- a **reconciler** (`tv-rs-agent`) that converges a fleet towards desired state,
  heals what it can and quarantines what it cannot, with an audit trail a technician
  can read.

The rules the code follows:

- **Discovery needs no credentials.** SSDP is plain LAN multicast.
- **Control is paired once, then remembered.** Samsung returns a token, LG a
  `client-key`; both are persisted and reused. Self-signed TV certificates are
  handled by pinning, not by disabling validation.
- **Nothing panics in library code.** A capability a TV does not have is a typed
  error, never an `unimplemented!()` or an `unwrap()`.
- **Refusals are part of the API.** Capability sets say in advance what will be
  refused; the same set drives the CLI, the facade and the tests.
- **No vendor SDK ships with it.** Every protocol is spoken directly over sockets —
  no bundled blob, no build-time download.

## What it does

|                   |                                                                                                                                                                                                                                                                  |
| ----------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
|                   |                                                                                                                                                                                                                                                                  |
| **Discover**      | SSDP `M-SEARCH` on UDP 1900, UPnP descriptor parsing, vendor identification                                                                                                                                                                                      |
| **Wake**          | Wake-on-LAN magic packet, plus a TCP probe that tells `On` / `Standby` / `Off` apart                                                                                                                                                                             |
| **Samsung Tizen** | remote WebSocket channel (8001 plain, 8002 TLS, negotiated automatically), DIAL launch (8080), pairing-token store, per-TV connection pool                                                                                                                       |
| **Samsung MDC**   | commercial-panel commands over TCP 1515: power, volume/mute, input, brightness, status, panel lock, content orientation, panel health (no-signal/lamp/temperature), panel hours; one socket can address a video wall by display ID                               |
| **LG webOS**      | SSAP over WebSocket (3000/3001, negotiated), pairing, `client-key` persistence, volume/mute/launch/power/foreground, `open_url` in the built-in browser, `switch_input`                                                                                          |
| **LG serial**     | commercial ASCII `ka`/`mc` over TCP 9761 or RS-232 (`tv-rs-lgcmd`)                                                                                                                                                                                               |
| **Philips SICP**  | commercial panels over TCP 5000 (`tv-rs-drivers`, feature `sicp`)                                                                                                                                                                                                |
| **Roku**          | unauthenticated ECP REST (8060): device info, apps, key presses, launch, power mode                                                                                                                                                                              |
| **Google Cast**   | sender over TCP 8009 (TLS + EKE2, protobuf): app launch, volume, media_status query, transport controls (play/pause/seek), DIAL HTTP fallback (8008)                                                                                                             |
| **HDMI-CEC**      | power on / standby / active source over the cable (subprocess `cec-client`, or platform-native FFI with `cec-native`)                                                                                                                                            |
| **Capabilities**  | per-vendor capability sets + `DeviceHandle` traits, so a fleet holds `Arc<dyn DeviceHandle>` with no generics; the LFD subset (`as_panel_lock()`, `as_orientation()`, `as_panel_health()`) is honest per backend — MDC says yes, consumer channels return `None` |
| **Confidence**    | `power_observation()` reports the state _and_ how it was learned — read (`High`) or probed (`Inferred`)                                                                                                                                                          |
| **Quirks**        | firmware/model defects as TOML; with the `quirks` feature a matching row removes a dead port from the ladder                                                                                                                                                     |
| **Automation**    | rules, scenes, schedules, cooldowns and conflict resolution (`tv-rs-rules`), grounded in the quirks RAG (`tv-rs-rag`)                                                                                                                                            |
| **Reconcile**     | desired state from a file, an HTTP manifest or MQTT; one pure decision function; healing ladder; JSONL audit log                                                                                                                                                 |
| **Vision**        | RTSP streams, edge AI models (YOLO, MediaPipe), zone analytics, JSON events (`tv-rs-vision`)                                                                                                                                                                     |
| **Mirroring**     | NectarLink v0 video-session vocabulary (`tv-rs-mirror`)                                                                                                                                                                                                          |
| **Spatial audio** | Bluetooth A2DP sinks, PulseAudio/PipeWire streams, directional sound domes (`tv-rs-spatial-audio`)                                                                                                                                                               |
| **Bluetooth**     | HID keyboard and BLE remote emulation (`tv-rs-bluetooth`)                                                                                                                                                                                                        |
| **Provisioning**  | first-boot BLE Improv Wi-Fi and QR pairing (`tv-rs-provision`, `tv-rs-companion`)                                                                                                                                                                                |
| **TV apps**       | a supplied web/wasm payload in, a Tizen `.wgt` / webOS app directory out (`tv-rs-app`)                                                                                                                                                                           |
| **Serve media**   | HTTP `206 Partial Content` for seeking, plus HLS playlists — the two things TV players insist on                                                                                                                                                                 |
| **Test doubles**  | MDC and SICP mock servers speaking the recorded wire format (`tv-rs-sim`)                                                                                                                                                                                        |

## What it does not do

It does not decode video, render anything, or transcode. It returns URLs and issues
commands; what you do with the picture is your call. There is no AirPlay backend.
Google Cast and HDMI-CEC **are** implemented, behind `tv-rs-drivers` features.
Android TV Remote v2 exists as a workspace-only spike (`tv-rs-androidtv`,
`publish = false`) and is not wired into the facade. There is no input switching on
Tizen or Roku yet; webOS has `switch_input`. See [`NON_GOALS.md`](NON_GOALS.md).

Nothing here has been verified on hardware from the development host that produced
it — [`docs/08-hil-results.md`](docs/08-hil-results.md) is the table, and it is empty.

## Install

```toml
[dependencies]
tv-rs = "0.8"           # facade + the vendor protocols it dispatches to
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

## Want less?

Nineteen crates are published; depend on one:

- `tv-rs-core` — the device model only (no tokio in its graph, ever)
- `tv-rs-drivers` — every vendor protocol, behind feature flags
- `tv-rs-transport` — TLS policy, pinning and the budgeted WebSocket frame reader
- `tv-rs-local` — local EDID/connector probe for the host's own screen
- `tv-rs-dlna` — DLNA renderer transport (SSDP + AVTransport + GENA)
- `tv-rs-lgcmd` — LG commercial serial (`ka`/`mc`) over TCP 9761 or RS-232
- `tv-rs-media` — `206`/HLS serving
- `tv-rs-agent` — reconciliation, when you are driving more than one screen
- `tv-rs-app` — package a web/wasm payload as an installable TV app
- `tv-rs-vision`, `tv-rs-rules`, `tv-rs-rag`, `tv-rs-provision`, `tv-rs-companion`,
  `tv-rs-mirror`, `tv-rs-spatial-audio`, `tv-rs-bluetooth`, `tv-rs-sim` — the rest of
  the published set

## The crates

| Crate                 | Role                                                                                                                                         | Why it is separate                                                   |
| --------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------- |
| Crate                 | Role                                                                                                                                         | Why it is separate                                                   |
| `tv-rs-core`          | capability traits, `DeviceHandle`, `CommandId`, confidence-tagged state, error taxonomy, mock backend, conformance suite, quirks loader      | no tokio, no protocol: the part a CMS can embed                      |
| `tv-rs`               | facade: discovery orchestration, vendor identification, `TvClient` (which implements `DeviceHandle`)                                         | one dependency for the common case                                   |
| `tv-rs-drivers`       | every vendor protocol behind a feature flag: SSDP, Wake-on-LAN, Tizen, webOS, Roku ECP, Samsung MDC, Philips SICP, HDMI-CEC, IR, Google Cast | one crate to version; feature flags instead of ten dependencies      |
| `tv-rs-transport`     | TLS policy/pinning + budgeted WebSocket frame reader                                                                                         | one place for the TLS decision                                       |
| `tv-rs-local`         | EDID/connector/panel probe for the host's attached displays                                                                                  | host hardware, not LAN control; no Tauri or database                 |
| `tv-rs-dlna`          | DLNA renderer transport: SSDP, AVTransport SOAP, GENA eventing, SSDP advertisement                                                           | transport only — no media library, no server; that stays app-owned   |
| `tv-rs-lgcmd`         | LG commercial ASCII serial (`ka`/`mc`) over TCP 9761 or RS-232                                                                               | wired walls and older panels that have no SSAP                       |
| `tv-rs-media`         | `206` + HLS HTTP server                                                                                                                      | pulls `axum`; must never enter a protocol crate's graph              |
| `tv-rs-agent`         | providers, `decide()`, healer, graduated remediation ladder (dim → restart → reboot), audit log, run loop                                    | opinionated; the facade stays a library                              |
| `tv-rs-app`           | a supplied web/wasm payload in, a Tizen `.wgt` / webOS app directory out                                                                     | TV knowledge (manifests, bundle layout, D-pad keys), no player logic |
| `tv-rs-vision`        | RTSP streams, edge AI models, zone analytics, JSON events                                                                                    | camera side, independent of the control path                         |
| `tv-rs-rules`         | declarative automation: conditions, priorities, cooldowns, schedules, scenes                                                                 | turns events into fleet actions                                      |
| `tv-rs-rag`           | in-memory RAG: quirks corpus, remediation ledger, rule grammar                                                                               | grounding for the agent's healer and the rules compiler              |
| `tv-rs-provision`     | first-boot provisioning: BLE Improv Wi-Fi, QR payload                                                                                        | commissioning, not operation                                         |
| `tv-rs-companion`     | mobile companion core (no_std+alloc, UniFFI bindings, SAS pairing)                                                                           | the phone side of pairing                                            |
| `tv-rs-mirror`        | NectarLink v0 video-session vocabulary                                                                                                       | screen mirroring without a vendor SDK                                |
| `tv-rs-spatial-audio` | Bluetooth A2DP sinks, PulseAudio/PipeWire, sound domes                                                                                       | audio routing, not TV control                                        |
| `tv-rs-bluetooth`     | Bluetooth HID keyboard and BLE remote emulation                                                                                              | emulates a HID keyboard / remote for Apple TV, Fire TV, etc.         |
| `tv-rs-sim`           | MDC + SICP mock servers speaking the recorded wire format                                                                                    | hardware-free CI                                                     |

Not published: `tv-rs-cli` (the reference consumer), `tv-rs-androidtv` (spike), and
the ten deprecated `tv-rs-<vendor>` shims — see
[`docs/01-architecture.md` §Publishing](docs/01-architecture.md#publishing).

### When to reach for which

| You want to…                                      | Use                                                                   |
| ------------------------------------------------- | --------------------------------------------------------------------- |
| You want to…                                      | Use                                                                   |
| find whatever is on the LAN and control it        | `tv-rs` (facade)                                                      |
| control one known vendor, nothing else            | `tv-rs-drivers` with that vendor's feature                            |
| keep fifty screens on the intended content        | `tv-rs-agent` — desired state in, one pure decision function          |
| turn a TV on from cold                            | `tv-rs-drivers` (`power`) — it works without any control protocol     |
| identify the panel this machine is plugged into   | `tv-rs-local` — EDID, connector and TV likelihood, no network         |
| push a URL to a DLNA renderer and track its state | `tv-rs-dlna` — AVTransport + GENA, no library behind it               |
| control a panel that is not on the network at all | `tv-rs-drivers` (`cec`), if there is a CEC adapter                    |
| serve video to a TV                               | `tv-rs-media` — nothing else pulls `axum`                             |
| poke at a TV from a terminal                      | `cargo run -p tv-rs-cli`, or `cargo xtask hw-test` for a probe report |

## Layout

```
crates/core          device model      (package `tv-rs-core`)
crates/tv-rs         facade            (package `tv-rs`)
crates/drivers       every vendor protocol, feature-flagged (package `tv-rs-drivers`)
crates/transport     TLS + WS plumbing (package `tv-rs-transport`)
crates/local         display probe     (package `tv-rs-local`)
crates/dlna          DLNA transport    (package `tv-rs-dlna`)
crates/lgcmd         LG serial         (package `tv-rs-lgcmd`)
crates/media         206 + HLS server  (package `tv-rs-media`)
crates/agent         reconciliation    (package `tv-rs-agent`)
crates/app           TV app producer   (package `tv-rs-app`)
crates/vision        camera events     (package `tv-rs-vision`)
crates/rules         automation        (package `tv-rs-rules`)
crates/rag           quirks grounding  (package `tv-rs-rag`)
crates/provision     first-boot setup  (package `tv-rs-provision`)
crates/companion     phone-side core   (package `tv-rs-companion`)
crates/mirror        mirroring vocab   (package `tv-rs-mirror`)
crates/spatial-audio audio routing     (package `tv-rs-spatial-audio`)
crates/bluetooth     HID / BLE remote  (package `tv-rs-bluetooth`)
crates/sim           mock hardware     (package `tv-rs-sim`)
crates/{cast,cec,discovery,ir,mdc,power,roku,sicp,tizen,webos}
                     deprecated unpublished shims re-exporting `tv-rs-drivers`
crates/{cli,androidtv}
                     workspace-only: reference consumer and Android TV spike
quirks/              model+firmware defects, as data
xtask                `cargo xtask publish` · `cargo xtask hw-test`
```

The root `Cargo.toml` is a virtual workspace, so a bare `cargo build` builds
everything — name the package (`cargo build -p tv-rs`) unless you want that.

## How it fits together

```
              ┌──────────────────────────────┐
your app ────▶│ tv-rs (facade)               │
   or         │ TvClient: DeviceHandle        │
tv-rs-agent ─▶└──┬───────────────────────┬───┘
                  │                       │
        tv-rs-drivers (feature flags)     │
        tizen · webos · roku · cast ·     │
        cec · mdc · sicp · ir ·           │
        discovery · power                 │
                  │                       │
                  └───────────────────────┘
                              │
                    tv-rs-transport (TLS, pinning, frames)

tv-rs-core ── traits, state, errors, mock, conformance   (no tokio)
tv-rs-media ── axum, independent of everything above
```

The facade holds no protocol logic: it classifies a discovered device and forwards.
Vendor specifics stay reachable through `as_tizen()`, `as_webos()`, `as_roku()`,
`as_cast()`, `as_cec()` and `as_sicp()` on `TvClient`, through the re-exported
`tv_rs::<vendor>` modules, and through the capability traits when you do not know the
vendor.

## Feature flags

| Crate                 | Feature                                                      | Default | Effect                                                                                   |
| --------------------- | ------------------------------------------------------------ | ------- | ---------------------------------------------------------------------------------------- |
| Crate                 | Feature                                                      | Default | Effect                                                                                   |
| `tv-rs-core`          | `conformance`                                                | yes     | the suite every `DeviceHandle` must pass                                                 |
|                       | `mock-backend`                                               | no      | the scriptable backend every test uses                                                   |
|                       | `quirks`                                                     | no      | parse `quirks/default.toml`                                                              |
|                       | `supporting`                                                 | no      | the `supporting`-tier LFD surfaces: `LampOrPanelHours`, `ScreenCapture`                  |
|                       | `serde`                                                      | no      | serialize the device model as JSON                                                       |
| `tv-rs-drivers`       | `discovery`, `power`                                         | yes     | SSDP and Wake-on-LAN                                                                     |
|                       | `tizen`, `webos`, `roku`, `cast`, `cec`, `mdc`, `sicp`, `ir` | no      | one vendor protocol each                                                                 |
|                       | `mobile`                                                     | no      | subnet sweep helpers (`ipnet`)                                                           |
|                       | `serial`                                                     | no      | MDC over RS-232 instead of TCP 1515; same frames, same codec                             |
|                       | `cec-native`                                                 | no      | platform-native CEC transport instead of the `cec-client` subprocess                     |
|                       | `supporting`                                                 | no      | the LFD subset (`PanelHoursProvider`, `ScreenCapturer`)                                  |
|                       | `testkit`                                                    | no      | test doubles for the pooled Tizen/webOS clients                                          |
| `tv-rs-transport`     | `rustls`                                                     | yes     | pure-Rust TLS, no OpenSSL                                                                |
|                       | `native-tls`                                                 | no      | platform trust store instead; mutually exclusive with `rustls` (both = `compile_error!`) |
|                       | `p2p`                                                        | yes     | QUIC high-speed transport for signage asset transfer                                     |
| `tv-rs-agent`         | `http`                                                       | no      | `HttpPollProvider` (pulls `reqwest`)                                                     |
|                       | `mqtt`                                                       | no      | `MqttSubscribeProvider` (pulls `rumqttc`)                                                |
| `tv-rs`               | `cast`                                                       | no      | reserved; Google Cast arrives through `tv-rs-drivers/cast`                               |
|                       | `quirks`                                                     | no      | consult `quirks/default.toml` when building a client                                     |
|                       | `conformance`                                                | no      | run the shared suite against the facade's clients                                        |
|                       | `testkit`                                                    | no      | compile the Tizen/webOS test doubles                                                     |
|                       | `hil`                                                        | no      | compile the hardware-in-the-loop tests                                                   |
|                       | `supporting`                                                 | no      | the LFD surfaces most deployments never touch                                            |
| `tv-rs-companion`     | `uniffi-bindings`                                            | no      | Swift/Kotlin bindings                                                                    |
| `tv-rs-bluetooth`     | `bluer`, `btleplug`                                          | no      | the Bluetooth backend to use                                                             |
| `tv-rs-spatial-audio` | `serde`                                                      | no      | serialize the routing model                                                              |
| `tv-rs-provision`     | `serde`                                                      | yes     | serialize the provisioning payload                                                       |

## CLI

```bash
cargo run -p tv-rs-cli -- discover
cargo run -p tv-rs-cli -- status          # power + capability set per device
cargo run -p tv-rs-cli -- ensure --config desired.json --audit audit.jsonl
cargo run -p tv-rs-cli -- wake --mac AA:BB:CC:DD:EE:FF
cargo run -p tv-rs-cli -- power-off --ip 192.168.1.40 --vendor samsung
cargo run -p tv-rs-cli -- key --ip 192.168.1.40 --vendor roku --key home
cargo run -p tv-rs-cli -- launch --ip 192.168.1.41 --vendor roku --app netflix
```

`tv-rs-cli` is the facade's reference consumer and is **not published**: it exists so
there is one obvious way to drive the whole surface end to end.

`status` is the table to read before blaming the crate: it prints what each device
advertises, and an unsupported control shows up there before it shows up as an
error. `ensure` runs one reconciliation pass and exits non-zero if any device
failed, so a deploy script can gate on it.

## Testing

```bash
cargo test --workspace                          # no network needed
cargo test -p tv-rs --features conformance      # the shared device-model suite
cargo test -p tv-rs-core --features "mock-backend conformance quirks"
cargo test -p tv-rs-agent --features "http mqtt"
cargo xtask hw-test --timeout 5 --write-doc     # probe real televisions
cargo test -p tv-rs --features hil --test hardware_in_the_loop
```

`hw-test` discovers, probes power and capabilities, and appends to
[`docs/08-hil-results.md`](docs/08-hil-results.md). It reports "no televisions answered"
and exits 0 rather than inventing a pass.

`tv-rs-agent` also carries the Phase 4 exit criterion as a test: 50 devices, 24
simulated hours, three blips, two devices that never pair, one whose power is never
observable — every healthy device converges every hour, each blip costs exactly one
wake, and no device exceeds one action per hour.

Library code is safe: `#![forbid(unsafe_code)]` is on the protocol, facade and support
crates, and `tv-rs-local` isolates its Windows FFI behind `#![deny(unsafe_code)]` with
an `allow` on that one module. Dependencies are audited by `cargo-deny`; the MSRV is
checked by building it.

Gates CI enforces, and you should run before pushing:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

## Publishing

Nineteen crates go out in topological dependency order because `cargo publish` is not atomic — a
partial release burns the version:

```
tv-rs-core → tv-rs-transport → tv-rs-drivers → tv-rs-local
  → tv-rs-dlna → tv-rs-lgcmd → tv-rs-sim → tv-rs-app → tv-rs-mirror
  → tv-rs-spatial-audio → tv-rs-vision → tv-rs-rules → tv-rs-rag
  → tv-rs-provision → tv-rs-companion → tv-rs → tv-rs-media
  → tv-rs-agent → tv-rs-bluetooth
```

`core` leads because every backend depends on its vocabulary; `local`, `media`,
`agent` and `dlna` are independent leaves. The list lives in `xtask/src/main.rs` so
`cargo` can check that every name still exists. The deprecated `tv-rs-<vendor>` crates
are workspace-only shims and are never published.

```bash
cargo xtask publish --dry-run              # verify all nineteen, upload nothing
cargo xtask publish --yes                  # real publish, token from $CRATES_IO_TOKEN
cargo xtask publish --version 0.3.0 --yes  # bump every pin, then publish in order
```

The publisher is idempotent on purpose: a version already on crates.io is skipped,
and a dependent that cannot resolve yet is retried four times. A first publish
therefore survives the index being a minute behind, and a re-run after a failure
picks up where it stopped instead of failing on crate #1.

## Docs

New here? [`docs/00-index.md`](docs/00-index.md) is the map — every term links to
the page that explains it.

| Start with                                               | If you want to                                                                |
| -------------------------------------------------------- | ----------------------------------------------------------------------------- |
| Start with                                               | If you want to                                                                |
| [`docs/09-change-classes.md`](docs/09-change-classes.md) | **is this change breaking?** classify it before you write it                  |
| [`docs/02-playbook.md`](docs/02-playbook.md)             | the protocol facts: ports, handshakes, what each vendor exposes               |
| [`docs/01-architecture.md`](docs/01-architecture.md)     | the layers, the dependency rule, where a new file belongs                     |
| [`INTEGRATION.md`](INTEGRATION.md)                       | **put it in an application**: discovery, pinning, quirks, a fleet, media, CEC |
| [`docs/03-matrix.md`](docs/03-matrix.md)                 | what is done, what is deferred, verified test counts                          |
| [`docs/06-plan.md`](docs/06-plan.md)                     | **the live plan**: only open work, with exit criteria                         |
| [`QUIRKS.md`](QUIRKS.md)                                 | how to submit a model/firmware defect                                         |
| [`docs/04-external-refs.md`](docs/04-external-refs.md)   | protocol references and prior art                                             |

## Contributing

[`CONTRIBUTING.md`](CONTRIBUTING.md) has the gates, the module layout rule and the
commit convention; [`docs/09-change-classes.md`](docs/09-change-classes.md) classifies
a change before you make it. [`SECURITY.md`](SECURITY.md) covers disclosure and the LAN
threat model. [`NON_GOALS.md`](NON_GOALS.md) is what will be refused.

## License

MIT — see [`LICENSE`](LICENSE). One licence at the root; each manifest carries the
SPDX identifier.
