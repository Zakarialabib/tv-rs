# tv-rs

[![CI](https://github.com/Zakarialabib/tv-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/Zakarialabib/tv-rs/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/tv-rs.svg)](https://crates.io/crates/tv-rs)
[![docs.rs](https://docs.rs/tv-rs/badge.svg)](https://docs.rs/tv-rs)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![MSRV 1.89](https://img.shields.io/badge/MSRV-1.89-blue.svg)](Cargo.toml)

Find the televisions on your LAN and drive them from Rust — discovery, power,
remote control, fleet reconciliation and media serving, behind one small facade.

> **Version check.** `0.2.0` is on crates.io for thirteen crates; `tv-rs-mdc` is
> merged in this repository but not yet published. The surface this page describes —
> `power_observation()`, the `quirks` feature, `TizenRemote`, `tv-rs-local`,
> `tv-rs-dlna` — first appears in `0.2.0`; if you pin an older version, check
> [docs.rs](https://docs.rs/tv-rs) for what that version actually has.

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
of XML, a third panics when the TV is asleep. This workspace puts each protocol in
its own crate with the same shape, then adds two layers on top:

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

|                   |                                                                                                                                                         |
| ----------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Discover**      | SSDP `M-SEARCH` on UDP 1900, UPnP descriptor parsing, vendor identification                                                                             |
| **Wake**          | Wake-on-LAN magic packet, plus a TCP probe that tells `On` / `Standby` / `Off` apart                                                                    |
| **Samsung Tizen** | remote WebSocket channel (8001 plain, 8002 TLS, negotiated automatically), DIAL launch (8080), pairing-token store, per-TV connection pool              |
| **Samsung MDC**   | serial-style command frames over TCP 1515: power, volume/mute, input, brightness and status; one socket can address a video wall by display ID          |
| **LG webOS**      | SSAP over WebSocket (3000/3001, negotiated), pairing, `client-key` persistence, volume/mute/launch/power/foreground, `open_url` in the built-in browser |
| **Roku**          | unauthenticated ECP REST (8060): device info, apps, key presses, launch, power mode                                                                     |
| **Capabilities**  | per-vendor capability sets + `DeviceHandle` traits, so a fleet holds `Arc<dyn DeviceHandle>` with no generics                                           |
| **Confidence**    | `power_observation()` reports the state *and* how it was learned — read (`High`) or probed (`Inferred`)                                                 |
| **Quirks**        | firmware/model defects as TOML; with the `quirks` feature a matching row removes a dead port from the ladder                                            |
| **Reconcile**     | desired state from a file, an HTTP manifest or MQTT; one pure decision function; healing ladder; JSONL audit log                                        |
| **HDMI-CEC**      | power on / standby / active source over the cable, for panels the network cannot reach (optional)                                                       |
| **Serve media**   | HTTP `206 Partial Content` for seeking, plus HLS playlists — the two things TV players insist on                                                        |

## What it does not do

It does not decode video, render anything, or transcode. It returns URLs and issues
commands; what you do with the picture is your call. There is no Android TV, Google
Cast or AirPlay backend yet, no event subscriptions (the agent polls), and no
input switching on Tizen, webOS or Roku yet. See [`NON_GOALS.md`](NON_GOALS.md).

Nothing here has been verified on hardware from the development host that produced
it — [`docs/hil-results.md`](docs/hil-results.md) is the table, and it is empty.

## Install

```toml
[dependencies]
tv-rs = "0.2"           # facade + the protocol crates it dispatches to
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

Want less? Depend on one crate:

- `tv-rs-core` — the device model only (no tokio in its graph, ever)
- `tv-rs-tizen`, `tv-rs-webos`, `tv-rs-roku`, `tv-rs-mdc` — one vendor each
- `tv-rs-discovery`, `tv-rs-power` — SSDP and Wake-on-LAN
- `tv-rs-local` — local EDID/connector probe for the host's own screen
- `tv-rs-dlna` — DLNA renderer transport (SSDP + AVTransport + GENA)
- `tv-rs-agent` — reconciliation, when you are driving more than one screen
- `tv-rs-cec` — HDMI-CEC, behind the `client` feature
- `tv-rs-media` — `206`/HLS serving

## The crates

| Crate             | Role                                                                                                                                    | Why it is separate                                                 |
| ----------------- | --------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------ |
| `tv-rs-core`      | capability traits, `DeviceHandle`, `CommandId`, confidence-tagged state, error taxonomy, mock backend, conformance suite, quirks loader | no tokio, no protocol: the part a CMS can embed                    |
| `tv-rs`           | facade: discovery orchestration, vendor identification, `TvClient` (which implements `DeviceHandle`)                                    | one dependency for the common case                                 |
| `tv-rs-discovery` | SSDP + UPnP descriptors                                                                                                                 | no TLS, no vendor knowledge                                        |
| `tv-rs-power`     | Wake-on-LAN + power probe                                                                                                               | useful without any control protocol                                |
| `tv-rs-local`     | EDID/connector/panel probe for the host's attached displays                                                                             | host hardware, not LAN control; no Tauri or database               |
| `tv-rs-dlna`      | DLNA renderer transport: SSDP, AVTransport SOAP, GENA eventing, SSDP advertisement                                                      | transport only — no media library, no server; that stays app-owned |
| `tv-rs-tizen`     | Samsung remote channel, DIAL, token store, connection pool                                                                              | largest protocol surface                                           |
| `tv-rs-webos`     | LG SSAP                                                                                                                                 | different framing, different pairing                               |
| `tv-rs-roku`      | Roku ECP                                                                                                                                | plain REST, no auth at all                                         |
| `tv-rs-mdc`       | Samsung MDC over TCP 1515: power, volume/mute, input, brightness, status; shared socket for walls                                       | different wire protocol from Tizen; no WebSocket or TLS            |
| `tv-rs-transport` | TLS policy/pinning + budgeted WebSocket frame reader                                                                                    | one place for the TLS decision                                     |
| `tv-rs-agent`     | providers, `decide()`, healer, audit log, run loop                                                                                      | opinionated; the facade stays a library                            |
| `tv-rs-cec`       | HDMI-CEC via `cec-client` (optional)                                                                                                    | needs an adapter most machines lack                                |
| `tv-rs-media`     | `206` + HLS HTTP server                                                                                                                 | pulls `axum`; must never enter a protocol crate's graph            |
| `tv-rs-cli`       | `discover`, `status`, `ensure`, `wake`, `power-off`, `key`, `launch`                                                                    | the facade's reference consumer (`publish = false`)                |

### When to reach for which

| You want to…                                      | Use                                                                                    |
| ------------------------------------------------- | -------------------------------------------------------------------------------------- |
| find whatever is on the LAN and control it        | `tv-rs` (facade)                                                                       |
| control one known vendor, nothing else            | that vendor's crate                                                                    |
| keep fifty screens on the intended content        | `tv-rs-agent` — and read §12 of the desktop roadmap before writing your own retry loop |
| turn a TV on from cold                            | `tv-rs-power` — it works without any control protocol                                  |
| identify the panel this machine is plugged into   | `tv-rs-local` — EDID, connector and TV likelihood, no network                          |
| push a URL to a DLNA renderer and track its state | `tv-rs-dlna` — AVTransport + GENA, no library behind it                                |
| control a panel that is not on the network at all | `tv-rs-cec`, if there is a CEC adapter                                                 |
| serve video to a TV                               | `tv-rs-media` — nothing else pulls `axum`                                              |
| poke at a TV from a terminal                      | `tv-rs-cli`, or `cargo xtask hw-test` for a probe report                               |

## Layout

```
crates/core          device model      (package `tv-rs-core`)
crates/tv-rs         facade            (package `tv-rs`)
crates/discovery     SSDP              (package `tv-rs-discovery`)
crates/power         Wake-on-LAN       (package `tv-rs-power`)
crates/local         display probe     (package `tv-rs-local`)
crates/dlna          DLNA transport    (package `tv-rs-dlna`)
crates/tizen         Samsung           (package `tv-rs-tizen`)
crates/webos         LG                (package `tv-rs-webos`)
crates/roku          Roku              (package `tv-rs-roku`)
crates/mdc           Samsung MDC       (package `tv-rs-mdc`)
crates/transport     TLS + WS plumbing (package `tv-rs-transport`)
crates/media         206 + HLS server  (package `tv-rs-media`)
crates/agent         reconciliation    (package `tv-rs-agent`)
crates/cec           HDMI-CEC          (package `tv-rs-cec`)
crates/cli           terminal front end (package `tv-rs-cli`, not published)
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
tv-rs-agent ─▶└──┬───────────┬───────────┬───┘
                  │           │           │
            tv-rs-tizen  tv-rs-webos  tv-rs-roku
                  └───────────┴───────────┘
                              │
                    tv-rs-transport (TLS, pinning, frames)

tv-rs-core ── traits, state, errors, mock, conformance   (no tokio)
tv-rs-media ── axum, independent of everything above
tv-rs-cec ── HDMI, independent, optional
```

The facade holds no protocol logic: it classifies a discovered device and forwards.
Vendor specifics stay reachable through `as_tizen()`, `as_webos()` and `as_roku()`
on `TvClient`, and through the capability traits when you do not know the vendor.

## Feature flags

| Crate             | Feature        | Default | Effect                                                                                   |
| ----------------- | -------------- | ------- | ---------------------------------------------------------------------------------------- |
| `tv-rs-core`      | `mock-backend` | no      | the scriptable backend every test uses                                                   |
|                   | `conformance`  | no      | the suite every `DeviceHandle` must pass                                                 |
|                   | `quirks`       | no      | parse `quirks/default.toml`                                                              |
| `tv-rs-transport` | `rustls`       | yes     | pure-Rust TLS, no OpenSSL                                                                |
|                   | `native-tls`   | no      | platform trust store instead; mutually exclusive with `rustls` (both = `compile_error!`) |
| `tv-rs-agent`     | `http`         | no      | `HttpPollProvider` (pulls `reqwest`)                                                     |
|                   | `mqtt`         | no      | `MqttSubscribeProvider` (pulls `rumqttc`)                                                |
| `tv-rs-cec`       | `client`       | no      | shell out to `cec-client`                                                                |
| `tv-rs`           | `quirks`       | no      | consult `quirks/default.toml` when building a client                                     |
|                   | `hil`          | no      | compile the hardware-in-the-loop tests                                                   |
|                   | `conformance`  | no      | run the shared suite against the facade's clients                                        |

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
[`docs/hil-results.md`](docs/hil-results.md). It reports "no televisions answered"
and exits 0 rather than inventing a pass.

`tv-rs-agent` also carries the Phase 4 exit criterion as a test: 50 devices, 24
simulated hours, three blips, two devices that never pair, one whose power is never
observable — every healthy device converges every hour, each blip costs exactly one
wake, and no device exceeds one action per hour.

`#![forbid(unsafe_code)]` is on every crate. Dependencies are audited by
`cargo-deny`; the MSRV is checked by building it.

Gates CI enforces, and you should run before pushing:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

Plus `cargo-deny check` for advisories/licences, and an `msrv` job that builds
against the declared `rust-version` (1.89 — the lowest version that actually
resolves, not a target chosen for reach).

## Publishing

Fourteen crates go out in dependency order because `cargo publish` is not atomic — a
partial release burns the version:

```
tv-rs-core → tv-rs-transport → tv-rs-discovery → tv-rs-power → tv-rs-local
  → tv-rs-dlna → tv-rs-roku → tv-rs-mdc → tv-rs-tizen → tv-rs-webos → tv-rs
  → tv-rs-media → tv-rs-agent → tv-rs-cec
```

`core` leads because every backend depends on its vocabulary; `local`, `media`,
`agent` and `cec` are independent leaves. The list lives in `xtask/src/main.rs` so
`cargo` can check that every name still exists.

```bash
cargo xtask publish --dry-run              # verify all fourteen, upload nothing
cargo xtask publish --yes                  # real publish, token from $CRATES_IO_TOKEN
cargo xtask publish --version 0.2.0 --yes  # bump every pin, then publish in order
```

The publisher is idempotent on purpose: a version already on crates.io is skipped,
and a dependent that cannot resolve yet is retried four times. A first publish
therefore survives the index being a minute behind, and a re-run after a failure
picks up where it stopped instead of failing on crate #1.

## Docs

New here? [`docs/glossary.md`](docs/glossary.md) is the map — every term links to
the page that explains it.

| Start with                                                         | If you want to                                                                |
| ------------------------------------------------------------------ | ----------------------------------------------------------------------------- |
| [`docs/01-playbook.md`](docs/01-playbook.md)                       | the protocol facts: ports, handshakes, what each vendor exposes               |
| [`docs/architecture.md`](docs/architecture.md)                     | the three layers, the dependency rule, where a new file belongs               |
| [`INTEGRATION.md`](INTEGRATION.md)                                 | **put it in an application**: discovery, pinning, quirks, a fleet, media, CEC |
| [`docs/03-matrix.md`](docs/03-matrix.md)                           | what is done, what is deferred, verified test counts                          |
| [`docs/06-plan.md`](docs/06-plan.md)                         | **the live plan**: only open work, with exit criteria          |
| [`QUIRKS.md`](QUIRKS.md)                                           | how to submit a model/firmware defect                                         |
| [`docs/02-external-refs.md`](docs/02-external-refs.md)             | protocol references and prior art                                             |

## Contributing

[`CONTRIBUTING.md`](CONTRIBUTING.md) has the gates, the module layout rule and the
commit convention. [`SECURITY.md`](SECURITY.md) covers disclosure and the LAN threat
model. [`NON_GOALS.md`](NON_GOALS.md) is what will be refused.

## License

MIT — see [`LICENSE`](LICENSE). One licence at the root; each manifest carries the
SPDX identifier.
