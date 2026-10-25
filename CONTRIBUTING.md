# Contributing

## Prerequisites

- Rust 1.89 or newer (`rust-version` in `Cargo.toml`; CI builds that exact version).
- `rustfmt` and `clippy` components installed — CI fails without them, and a
  host that lacks them silently produces a passing-looking local run.

```bash
rustup component add rustfmt clippy
```

## Gates

Branch protection requires five checks — rustfmt, clippy, tests on Linux, tests on
Windows, rustdoc. Run the same four commands before pushing:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

CI runs four more jobs that are not branch protection but are still red builds you
will be asked to fix:

| Job                        | What it catches that a local `--workspace` run does not                      |
| -------------------------- | ---------------------------------------------------------------------------- |
| `msrv (1.89.0)`            | a `rust-version` nobody builds against, and code that needs a newer compiler |
| `cargo-deny`               | advisories, unknown sources, wildcard deps, licences outside `deny.toml`     |
| `per-crate features`       | a crate that only compiles because a sibling turned a feature on             |
| `core (tokio-free + mock)` | tokio sneaking into `tv-rs-core`'s graph                                     |

The last one is the reason the `--workspace` runs are not enough: feature unification
hides exactly the break that job exists to find.

Hardware tests are opt-in and must never run in CI:

```bash
TV_RS_HIL_ROKU_HOST=192.168.1.40 cargo test --features hil --test hardware_in_the_loop
```

## Layout rules

Each protocol crate follows the same module shape — see
[`docs/01-architecture.md`](docs/01-architecture.md):

```
<protocol>/src/{lib,auth,protocol,payload,types,client,error}.rs
```

- `auth.rs` holds the authentication trait. Anything that depends on
  undocumented vendor behaviour returns a typed `Unsupported` error; library
  code must never panic and must never use `unimplemented!()`.
- `payload.rs` holds the serde/XML structs, named
  `<Protocol><Payload>`.
- Pure parsing and formatting live in free functions so they test without a
  socket.

## Commit convention

[Conventional Commits](https://www.conventionalcommits.org/), scoped by crate
where it helps, one logical change per commit:

```
feat(tizen): add DIAL app launch
fix(webos): persist the client-key after pairing
docs: reconcile the CCTV integration spec against src
chore(lint): satisfy clippy -D warnings
```

Group related files in a commit; do not mix formatting churn with behaviour.

## Versioning policy

`0.x` until the error taxonomy and the capability traits have been stable for a full
release cycle; from `1.0`, plain [SemVer](https://semver.org/spec/v2.0.0.html).

What counts as breaking while at `0.x`, and needs a minor bump (`0.1` → `0.2`) plus a
CHANGELOG entry under **Changed** or **Removed**:

- adding a **variant to an enum a caller matches on** without `_ =>` — `TvError`,
  `PowerState`, `TvVendor`, `Confidence`, `Decision`. Adding a variant is additive for
  the crate and breaking for the caller, and pretending otherwise is how a patch
  release breaks a build;
- removing or renaming a public item, or changing a method's argument order;
- changing a default that alters behaviour (a port order, a timeout, a confidence);
- raising `rust-version`.

Not breaking: a new capability on a backend, a new defaulted method on a trait, a new
quirk row, a new feature flag (off by default), a new field on a struct that is
`#[non_exhaustive]` or not constructed by callers.

**The error taxonomy is the strictest surface.** Once `0.2` ships, variants of
`TvError` may not be removed or repurposed for at least 90 days, because `Remedy`
mapping lives in downstream retry code. Add a variant, add its remedy, add the
exhaustiveness test case — in the same PR.

All fourteen crates are versioned together and published together: `cargo xtask
publish --version X.Y.Z` rewrites the pins. A partial release is worse than a
late one.

## Releasing

The crates publish in dependency order via `cargo xtask publish` — the order is
`xtask/src/main.rs::ORDER`, recorded with its rationale in
[`docs/01-architecture.md` §Publishing](docs/01-architecture.md#publishing).
crates.io yanks rather than deletes, so a partial publish burns the version —
dry-run first:

```bash
cargo xtask publish --dry-run
cargo xtask publish --version 0.3.0
```

## Docs

Start at [`docs/00-index.md`](docs/00-index.md). `docs/03-matrix.md` is the
authoritative status document; update it when a status row changes.
