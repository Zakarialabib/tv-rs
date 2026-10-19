# tv-rs — TV Control & Communications

Cross-platform TV discovery and control in Rust. One facade crate finds
televisions on the local network and drives them regardless of vendor; the
protocol work lives in five focused crates beneath it.

**Status:** Phase 1 complete — discovery, Wake-on-LAN, and the Samsung Tizen,
LG webOS and Roku control channels are implemented and tested. The
reverse-engineered authentication layers (Playbook §10 Gaps A–E) are deliberately
unimplemented and fail closed, never panic.

## Crates

| Crate | Playbook § | Responsibility |
|---|---|---|
| `tv-rs` | — | Facade: discovery orchestration, vendor identification, aggregate client |
| `tv-rs-discovery` | §1 | SSDP `M-SEARCH` (UDP 1900) and UPnP device descriptor parsing |
| `tv-rs-power` | §2 | Wake-on-LAN magic packets (UDP 9) and power-state probing |
| `tv-rs-tizen` | §3 | Samsung Tizen remote channel (WS 8001/8002) and DIAL launcher (8080) |
| `tv-rs-webos` | §4 | LG webOS SSAP over WebSocket (3000/3001) |
| `tv-rs-roku` | §5 | Roku ECP REST (8060) |
| `tv-rs-transport` | — | Shared transport vocabulary: TLS policy/connector, budgeted WebSocket frame reader, deferred-capability error |
| `tv-rs-media` | §7–8 | Range-capable HTTP media server: `206 Partial Content` and HLS manifests |
| `tv-rs-cli` | — | Command-line consumer of the facade (`discover`, `wake`, `power-off`, `key`, `launch`) — `publish = false` |

Discovery needs no credentials — SSDP is an unauthenticated LAN multicast
protocol. Roku control also needs none. Tizen and webOS prompt on the TV the
first time and issue a token / `client-key` to reuse afterwards.

## Usage

```toml
[dependencies]
tv-rs = "0.1"
```

```rust,no_run
use tv_rs::{discover, TvVendor};

# async fn run() -> tv_rs::Result<()> {
for tv in discover().await? {
    println!("{} ({}) at {}", tv.label(), tv.vendor.as_str(), tv.ip);

    if tv.vendor != TvVendor::Unknown {
        let client = tv.to_client()?;
        client.power_off().await?;
    }
}
# Ok(())
# }
```

The facade covers only the operations with an equivalent on every vendor.
Everything else — webOS volume control, Roku app listing, Tizen raw frames — is
reached through the re-exported protocol crates (`tv_rs::webos`, `tv_rs::roku`,
`tv_rs::tizen`, `tv_rs::discovery`, `tv_rs::power`) or the typed accessors on
`TvClient` (`as_tizen`, `as_webos`, `as_roku`).

## Build & test

```bash
cargo check --workspace --all-targets
cargo test --workspace

# Hardware-in-the-loop (never runs by default):
TV_RS_HIL_ROKU_HOST=192.168.1.40 cargo test --features hil --test hardware_in_the_loop

# CLI
cargo run -p tv-rs-cli -- discover
```

`cargo test` compiles the HIL suite to zero tests unless `--features hil` is
passed; the tests then skip any vendor whose `TV_RS_HIL_*` host is unset.

## Not implemented (Playbook §10, REA-gated)

| Gap | Topic | Blocked crate |
|---|---|---|
| A | TLS certificate-pinning bypass for self-signed TV certificates | `tizen`, `webos` |
| B | Enterprise token-generation math (MAC + secret salt) | `tizen` |
| C | Undocumented deep-link intents and payloads | `roku`, `webos` |
| D | AES-GCM encrypted WebSocket framing on newer firmware | `tizen` |
| E | Proprietary protobuf schemas | not yet a crate |

Also out of scope for Phase 1: Android TV ADB (5555, §6), DLNA/HLS streaming
(§7–8), Google Cast / AirPlay (§9). These are planned as additional protocol
crates; see `docs/03-matrix.md`.

Each blocked capability returns a typed error (`TizenError::Unsupported`,
`TvError::UnsupportedVendor`, `TvError::UnsupportedOperation`) rather than
panicking, so a consumer's process is never aborted.

## Documentation

Start at [`docs/00-index.md`](docs/00-index.md).

- [`docs/04-rust-package-standard.md`](docs/04-rust-package-standard.md) — the
  reusable engineering standard this project follows: repository shape, facade
  pattern, error contract, testability, naming, docs structure, dependency policy,
  CI gates and publishing. An identical copy lives in `ezviz-openapi-rs`.
- [`docs/03-matrix.md`](docs/03-matrix.md) — the state matrix: what is done, what
  is left, verified test counts, and the Phase 2 & 3 roadmap.

The remainder of `docs/` covers this project specifically: the protocol playbook,
external references, the publication ADR, a glossary, and the Playbook-to-crate
map.

## Publishing

Eight crates publish to crates.io. Publishing is **not** atomic — crates.io
yanks rather than deletes — so releases go in dependency order via
[`scripts/release.sh`](scripts/release.sh):

```
tv-rs-transport → tv-rs-discovery → tv-rs-power → tv-rs-roku → tv-rs-tizen
  → tv-rs-webos → tv-rs → tv-rs-media
```

`tv-rs-transport` goes first because every other workspace crate builds on it:
its `Unsupported` error vocabulary is unconditional, while the TLS connector and
WebSocket frame reader sit behind the default-off `ws` feature that only
`tizen`/`webos` enable.

```bash
bash scripts/release.sh --dry-run              # verify all eight, upload nothing
bash scripts/release.sh --version 0.2.0        # bump, then publish in order
```

The script verifies `LICENSE`, runs `cargo check --workspace --all-targets`,
refuses a dirty tree for a real publish, and bumps `[workspace.package]` plus
every version-pinned path dependency. CI (`.github/workflows/release.yml`)
calls it rather than a bare `cargo publish`.

## License

MIT — see [`LICENSE`](LICENSE).
