# ADR-002 — Workspace layout: one `crates/` tree, virtual root

Status: accepted · 2026-10-19

## Context

The workspace grew by accretion and ended up with three code homes and two
misleading folder names:

```
src/            the facade package (root package)
core/           discovery, power, transport
crates/         tizen, webos, roku, media
utils/cli/      the CLI package
utils/scripts/  release.sh
```

Four problems:

1. **`core/` was a misnomer.** Discovery and power are *protocol* crates (SSDP and
   Wake-on-LAN are wire protocols), while transport is a *support* crate. Grouping
   them together implied a shared role they do not have.
2. **`crates/` mixed roles.** It held vendor crates and the media server, so
   "in `crates/`" said nothing about what a crate was.
3. **The facade hid in root `src/`.** It is a ninth package, but it looked like a
   repository-root script folder, and `cargo build` at the root silently meant
   "build the facade only".
4. **Tooling lived in two idioms.** A shell script sat next to a Rust package in
   `utils/`, and the publish order — a property of the crate graph — was encoded
   in shell rather than next to the graph itself.

## Decision

1. **Flat `crates/` tree.** Every package lives at `crates/<dir>`, directories
   unprefixed, packages keeping the `tv-rs-` prefix. The facade is
   `crates/tv-rs` → package `tv-rs`.
2. **Virtual workspace root.** `Cargo.toml` has no `[package]`; members are
   `["crates/*", "xtask"]` — one glob, no special cases. The facade's manifest,
   features (`hil`) and dev-dependencies moved to `crates/tv-rs/Cargo.toml`.
3. **CLI promoted** from `utils/cli` to `crates/cli`; it is a member like any
   other, still `publish = false`.
4. **Release tooling is `xtask`.** `cargo xtask publish [--dry-run] [--version X]`
   replaces `utils/scripts/release.sh`; the publish order lives next to the code it
   describes and `cargo` can verify each package still exists.
5. **Layering is documented, not imposed by folders.** The facade/protocol/support
   model and the "protocol crates depend only on transport" rule are in
   [`../architecture.md`](../architecture.md); the dependency graph enforces them,
   and a folder split would have added a second, weaker copy of the same idea.
6. **`missing_docs = "deny"` moved to `[workspace.lints.rust]`.** One place to
   enforce it instead of nine identical crate-root attributes.

## Consequences

- One home per package: `git status`, `tree -L 2` and `cargo metadata` agree.
- The facade must be addressed explicitly: `cargo run -p tv-rs` rather than a bare
  `cargo run` at the root. The CLI keeps the friendly `cargo run -p tv-rs-cli`.
- Moving the facade into `crates/` changed every relative doc link inside it; links
  are relative and the crate root moved two levels deeper, so they were audited.
- `crates/tv-rs/tests/` now sits with its package; the `hil` feature travels with
  it. `cargo test --features hil` at the workspace root no longer applies to the
  facade implicitly.
- The relaxation is reversible per crate: a package that genuinely needs a
  different layout can still be added at its own path.

## Rejected alternatives

- **`crates/protocols/` + `crates/support/` subfolders.** Truthful, but it turns
  `members` into two-level globs, deviates from the near-universal Rust workspace
  convention, and buys nothing that `architecture.md` does not already say.
- **Facade stays a root package.** Keeps three code homes (`src/`, `core/`,
  `crates/`) and the misleading bare `cargo build`.
- **Keeping the shell script.** A script cannot know whether `tv-rs-tizen` still
  exists; the xtask's package list breaks the build when it drifts.