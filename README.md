# tv-rs

[![CI](https://github.com/Zakarialabib/tv-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/Zakarialabib/tv-rs/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![MSRV 1.75](https://img.shields.io/badge/MSRV-1.75-blue.svg)](Cargo.toml)

Find the televisions on your LAN and drive them from Rust — discovery, power,
remote control and media serving, behind one small facade.

```rust
use tv_rs::{discover, TvVendor};

for tv in discover().await? {
    println!("{} ({}) at {}", tv.label(), tv.vendor.as_str(), tv.ip);
    if tv.vendor != TvVendor::Unknown {
        tv.to_client()?.power_off().await?;
    }
}
```

## Why this exists

Every TV vendor speaks a different protocol on the same LAN, and none of the
libraries that wrap them agree on anything: one returns a socket, another a
future of XML, a third panics when the TV is asleep. This workspace puts each
protocol in its own crate with the same shape, and adds a facade that covers only
the operations every vendor actually has.

The rules the code follows:

- **Discovery needs no credentials.** SSDP is plain LAN multicast.
- **Control is paired once, then remembered.** Samsung returns a token, LG
  returns a `client-key`; both are persisted by the caller and reused.
- **Nothing panics in library code.** A capability a TV does not have is a typed
  error, not a `unimplemented!()` or an `unwrap()`.
- **No vendor SDK ships with it.** Every protocol here is spoken directly over
  sockets; there is no bundled blob and no build-time download.

## What it does

| | |
|---|---|
| **Discover** | SSDP `M-SEARCH` on UDP 1900, UPnP descriptor parsing, vendor identification |
| **Wake** | Wake-on-LAN magic packet, plus a TCP probe that tells `On` / `Standby` / `Off` apart |
| **Samsung Tizen** | remote WebSocket channel (8001 plain, 8002 TLS), DIAL app launch (8080), pairing-token store, per-TV connection pool |
| **LG webOS** | SSAP over WebSocket (3000/3001), pairing, `client-key` persistence, volume/mute/launch/power |
| **Roku** | unauthenticated ECP REST (8060): device info, apps, key presses, launch |
| **Serve media** | HTTP `206 Partial Content` for seeking, plus HLS playlists — the two things TV players insist on |

## What it does not do

It does not decode video, render anything, or transcode. It returns URLs and
issues commands; what you do with the picture is your call. It also does not
implement Android TV ADB or Google Cast/AirPlay.

A few vendor behaviours are deliberately unimplemented because they depend on
undocumented authentication (self-signed TLS pinning, enterprise token
derivation, encrypted framing). Those calls return `Unsupported` errors instead
of pretending to work.

## Install

```toml
[dependencies]
tv-rs = "0.1"           # facade + all protocol crates
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

Want less? Depend on one protocol crate directly — `tv-rs-tizen`, `tv-rs-webos`,
`tv-rs-roku`, `tv-rs-discovery`, `tv-rs-power` — or on `tv-rs-media` alone if you
only need the file server.

## The crates

| Crate | Role | Why it is separate |
|---|---|---|
| `tv-rs` | Facade: discovery orchestration, vendor identification, `TvClient` dispatch | one dependency for the common case |
| `tv-rs-discovery` | SSDP + UPnP descriptors | no TLS, no vendor knowledge |
| `tv-rs-power` | Wake-on-LAN + power probe | useful without any control protocol |
| `tv-rs-tizen` | Samsung remote channel, DIAL, token store, connection pool | largest protocol surface |
| `tv-rs-webos` | LG SSAP | different framing, different pairing |
| `tv-rs-roku` | Roku ECP | plain REST, no auth at all |
| `tv-rs-transport` | TLS policy/connector + budgeted WebSocket frame reader | shared vocabulary, one place for the TLS decision |
| `tv-rs-media` | `206` + HLS HTTP server | pulls `axum`; must never enter a protocol crate's graph |
| `tv-rs-cli` | `discover`, `wake`, `power-off`, `key`, `launch` | the facade's reference consumer (`publish = false`) |

### When to reach for which

| You want to… | Use |
|---|---|
| find whatever is on the LAN and control it | `tv-rs` (facade) |
| control one known vendor, nothing else | that vendor's crate (`tv-rs-tizen`, `tv-rs-webos`, `tv-rs-roku`) |
| turn a TV on from cold | `tv-rs-power` — it works without any control protocol |
| only enumerate devices | `tv-rs-discovery` — no TLS, no vendor knowledge |
| serve video to a TV | `tv-rs-media` — nothing else pulls `axum` |
| poke at a TV from a terminal | `tv-rs-cli` |

## Layout

```
crates/tv-rs         facade            (package `tv-rs`)
crates/discovery     SSDP              (package `tv-rs-discovery`)
crates/power         Wake-on-LAN       (package `tv-rs-power`)
crates/tizen         Samsung           (package `tv-rs-tizen`)
crates/webos         LG                (package `tv-rs-webos`)
crates/roku          Roku              (package `tv-rs-roku`)
crates/transport     TLS + WS plumbing (package `tv-rs-transport`)
crates/media         206 + HLS server  (package `tv-rs-media`)
crates/cli           terminal front end (package `tv-rs-cli`, not published)
xtask                `cargo xtask publish`
```

The root `Cargo.toml` is a virtual workspace, so a bare `cargo build` does nothing
useful — name the package (`cargo build -p tv-rs`) or use `--workspace`.

## How it fits together

```
your app ──▶ tv-rs (facade) ──▶ tv-rs-tizen ─┐
                             ├▶ tv-rs-webos  ├──▶ tv-rs-transport (TLS + WS)
                             ├▶ tv-rs-roku   │
                             └▶ tv-rs-discovery
              tv-rs-media ── (axum, independent)

```

The facade holds no protocol logic: it classifies a discovered device and
forwards. Anything vendor-specific is reached through `as_tizen()`,
`as_webos()` or `as_roku()` on `TvClient`.

## Feature flags

| Crate | Feature | Default | Effect |
|---|---|---|---|
| `tv-rs-transport` | `rustls` | yes | pure-Rust TLS, no OpenSSL |
| | `native-tls` | no | platform trust store instead; mutually exclusive with `rustls` |
| `tv-rs` | `hil` | no | compiles the hardware-in-the-loop tests (they run only with `--features hil`) |

## CLI

```bash
cargo run -p tv-rs-cli -- discover
cargo run -p tv-rs-cli -- wake --mac AA:BB:CC:DD:EE:FF
cargo run -p tv-rs-cli -- power-off --ip 192.168.1.40
cargo run -p tv-rs-cli -- key --ip 192.168.1.40 --key home
cargo run -p tv-rs-cli -- launch --ip 192.168.1.41 --app netflix
```

The CLI is also the best example of how the facade is meant to be used: it
matches on `TvError` and turns `UnsupportedOperation` into its own exit code.

## Testing

```bash
cargo test --workspace                 # unit + integration, no network needed
cargo test -p tv-rs --features hil --test hardware_in_the_loop   # against real TVs
```

The hardware suite compiles to zero tests unless `--features hil` is passed, and
each test skips with a printed reason when its `TV_RS_HIL_*` host variable is
unset. `TV_RS_HIL_STRICT=1` turns a missing TV into a failure for a lab runner.

Gates CI enforces, and you should run before pushing:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

## Publishing

Eight crates go out in dependency order because `cargo publish` is not atomic —
a partial release burns the version:

```
tv-rs-transport → tv-rs-discovery → tv-rs-power → tv-rs-roku
  → tv-rs-tizen → tv-rs-webos → tv-rs → tv-rs-media
```

`transport` leads because every protocol crate depends on its shared error
vocabulary; `media` is last because nothing here depends on it.

```bash
cargo xtask publish --dry-run          # verify all eight, upload nothing
cargo xtask publish --version 0.2.0    # bump and publish in order
```

## Docs

New here? [`docs/glossary.md`](docs/glossary.md) is the map — every term links to
the page that explains it. The short version:

| Start with | If you want to |
|---|---|
| [`docs/01-playbook.md`](docs/01-playbook.md) | the protocol facts: ports, handshakes, what each vendor exposes |
| [`docs/03-matrix.md`](docs/03-matrix.md) | what is done, what is deferred, and the verified test counts |
| [`docs/architecture.md`](docs/architecture.md) | the three layers, the dependency rule, and where a new file belongs |
| [`docs/04-rust-package-standard.md`](docs/04-rust-package-standard.md) | the engineering standard this workspace follows |
| [`docs/adr/001-crate-publication.md`](docs/adr/001-crate-publication.md) | why eight crates and how a release works |
| [`docs/02-external-refs.md`](docs/02-external-refs.md) | protocol references and prior art |

## Contributing

[`CONTRIBUTING.md`](CONTRIBUTING.md) has the gates, the module layout rule and
the commit convention. [`SECURITY.md`](SECURITY.md) covers disclosure.

## License

MIT — see [`LICENSE`](LICENSE).
