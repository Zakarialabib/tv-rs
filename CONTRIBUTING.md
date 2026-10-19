# Contributing

## Prerequisites

- Rust 1.75 or newer (`rust-version` in `Cargo.toml`).
- `rustfmt` and `clippy` components installed — CI fails without them, and a
  host that lacks them silently produces a passing-looking local run.

```bash
rustup component add rustfmt clippy
```

## Gates

Every pull request must pass the same five checks the branch protection
requires:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

Hardware tests are opt-in and must never run in CI:

```bash
TV_RS_HIL_ROKU_HOST=192.168.1.40 cargo test --features hil --test hardware_in_the_loop
```

## Layout rules

Each protocol crate follows the same module shape — see
[`docs/architecture.md`](docs/architecture.md):

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

## Releasing

Eight crates publish in dependency order via `cargo xtask publish`. crates.io
yanks rather than deletes, so a partial publish burns the version — dry-run
first:

```bash
cargo xtask publish --dry-run
cargo xtask publish --version 0.2.0
```

## Docs

Start at [`docs/00-index.md`](docs/00-index.md). `docs/03-matrix.md` is the
authoritative status document; update it when a status row changes.
