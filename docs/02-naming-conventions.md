# Naming & Layout — tv-rs Specifics

The universal rules live in [`04-rust-package-standard.md`](04-rust-package-standard.md)
§6 and §7. This file records only what is specific to this workspace: the actual
module inventory, and the deliberate deviations from the standard's vocabulary.

## Module Inventory

Which modules exist in which crate. The standard prescribes the vocabulary;
this table is the fact on disk.

| Crate | `error` | `types` | `payload` | `protocol` | `client` | `auth` |
|---|:--:|:--:|:--:|:--:|:--:|:--:|
| `tv-rs` (facade) | ● | ● | — | — | ● | — |
| `tv-rs-discovery` | ● | ● | ● | ● | ● | — |
| `tv-rs-power` | ● | ● | ● | ● | ● | — |
| `tv-rs-tizen` | ● | ● | ● | ● | ● | ● |
| `tv-rs-webos` | ● | ● | ● | ● | ● | ● |
| `tv-rs-roku` | ● | ● | ● | ● | ● | ● |

The facade has no `payload` or `protocol` module because it defines no wire
formats of its own; it dispatches to the crates that do.

**`tv-rs-transport` fits none of those columns**, because it is not a protocol
crate and has no `client`. It is named for what its modules carry rather than for
a layer: `tls.rs` (`TlsPolicy` and the connector — the only place in the
workspace where `danger_accept_invalid_certs` / `danger_accept_invalid_hostnames`
are called), `frame.rs` (`WsStream`, `ReadOutcome`, `read_until`) and
`unsupported.rs` (the `Unsupported { phase, reason }` struct that every protocol
crate's error enum wraps as `#[error(transparent)]`). The first two sit behind
the default-off `ws` feature, so `discovery`, `power` and `roku` depend on the
crate for the error vocabulary alone and never grow a TLS stack.

Consequence for the inventory above: `tizen` and `webos` no longer own a
`TlsPolicy` in `types.rs` — they re-export it from `tv_rs_transport`, so
`tv_rs::tizen::TlsPolicy` still resolves for callers.

## Deliberate Deviations

**`discovery` and `power` have no `auth.rs`.** SSDP and Wake-on-LAN are
unauthenticated protocols. An `auth.rs` in either crate would contain nothing but
a stub trait, so it was removed rather than kept for symmetry. Do not re-add it;
if a future protocol needs no credentials, omit the module.

**The facade uses a reduced module set** — `lib.rs`, `client.rs`, `error.rs`,
`types.rs`. It holds no protocol logic, so it has no wire-format modules.

## Package Name vs Directory Name

Both forms appear throughout these docs, deliberately:

- **Package name** (`tv-rs-tizen`) when discussing publishing, dependencies, or
  `use` statements. This is the name that appears on crates.io and as
  `tv_rs_tizen` in Rust source.
- **Directory name** (`tizen`, or `tizen/`) when discussing the source tree.

So `tizen/` and `tv-rs-tizen` are the same crate. `docs/03-matrix.md` uses
both in the same table; that is not an inconsistency.

## Facade Re-export Naming

The facade re-exports each protocol crate under a short module alias:

```rust
pub use tv_rs_discovery as discovery;
pub use tv_rs_power     as power;
pub use tv_rs_roku      as roku;
pub use tv_rs_tizen     as tizen;
pub use tv_rs_webos     as webos;
```

Alias drops the `tv_rs_` prefix, so users write `tv_rs::roku::RokuClient` rather
than `tv_rs::tv_rs_roku::RokuClient`. The alias is the directory name, which is
why the directory-name convention in the standard matters here.
