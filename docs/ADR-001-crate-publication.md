# ADR-001 — Crate Publication & Integration Mode

- **Status:** Accepted — partially implemented (see §Resolution)
- **Date:** 2026-10-18
- **Scope:** tv-rs workspace (facade + 5 protocol crates)
- **Supersedes:** —

## Context

The workspace declares `license = "MIT"` but originally shipped no `LICENSE`
file, which `cargo publish` rejects. `.github/workflows/release.yml` published
without a dry-run, without workspace-level tests, and without verifying the
license file.

All five protocol crates contain real implementations rather than stubs:
`protocol.rs` async traits (`#[async_trait]`), `client.rs` with real network code
(`tokio-tungstenite`, `reqwest`, `roxmltree`), `payload.rs` `serde` structs,
`types.rs` constants, and `error.rs` `thiserror` enums that cite the relevant
Playbook gap.

All dependencies are MIT-compatible (`tokio`, `serde`, `reqwest`, `thiserror`,
`futures-util`, `async-trait`, `roxmltree`, `base64`, `url`, `native-tls`,
`tokio-tungstenite`), so direct linking is fine and no sidecar isolation is needed.

## Decision

1. Create a `LICENSE` (MIT) file at the workspace root, matching the manifest.
2. Add `docs/glossary.md` for durable terminology.
3. Harden `.github/workflows/release.yml`: verify the license file, run
   `cargo test --workspace`, and dry-run the publish before publishing for real.
4. Link the Phase mapping in `docs/01-playbook.md` to crate versions in
   `docs/03-matrix.md`.
5. Never panic in library code. Blocked capabilities return typed errors
   (`TizenError::Unsupported`, `TvError::UnsupportedVendor`) instead of
   `unimplemented!()`, because a published crate must not abort a consumer's
   process.
6. Keep the crate-per-protocol split (see §Integration Mode).

## Resolution

| # | Item | Status |
|---|---|---|
| 1 | `LICENSE` (MIT) at workspace root | **Done** — file exists, `Copyright (c) 2026 Zakaria (tv-rs / signage-master)`. |
| 2 | `docs/glossary.md` | **Done** — terminology, protocol mapping, phase terms, license rules. |
| 3 | `release.yml` hardening | **Partially done** — license check, `cargo test --workspace`, and `--dry-run` are all in place. But see the unresolved blocker below. |
| 4 | Phase → crate version link | **Done** — `docs/03-matrix.md` maps every Playbook § to its crate and status. |
| 5 | No panics in library code | **Done** — no `unimplemented!()` / `todo!()` remains; the blocked paths return `Unsupported` variants. |
| 6 | Crate-per-protocol split | **Confirmed** — see §Integration Mode. |

## Unresolved blocker: `release.yml` cannot actually publish

`.github/workflows/release.yml` runs a single root-level `cargo publish`. That
publishes only the `tv-rs` facade. The five protocol crates are **not** on
crates.io, and Cargo strips `path` from the facade's dependencies at publish
time, turning them into registry requirements (`tv-rs-discovery = "0.1.0"`).
The publish will therefore fail with a missing-dependency error.

Correcting this requires publishing all eight packages in dependency order:

```
tv-rs-transport → tv-rs-discovery → tv-rs-power → tv-rs-roku → tv-rs-tizen
  → tv-rs-webos → tv-rs → tv-rs-media
```

`tv-rs-transport` leads because the five protocol crates depend on it for the
shared `Unsupported` error vocabulary; the facade (`tv-rs`) follows its
dependencies, and `tv-rs-media` is published last.

There is no stable `cargo publish --workspace`, and crates.io has **yank, not
delete** — so a partial failure burns the version already published. Options are
a release script, `cargo-release`, or `katyo/publish-crates`. This is tracked in
`docs/03-matrix.md` under Publish Readiness and is the single remaining task
before the first release.

## Consequences

**Positive.** Independent versioning per protocol crate; dependency isolation
(a `discovery`-only consumer does not compile the WebSocket stack); a new vendor
is a new crate plus one `pub use`; test isolation per crate; and a documented,
enforced error contract.

**Negative.** Eight packages to publish non-atomically; more `Cargo.toml` files
than a single crate; and the workspace compiles two TLS stacks — `native-tls`
(via `tokio-tungstenite`, for tizen/webos) and `rustls` (via `reqwest`, for
discovery/roku). A single crate with feature flags would give the same isolation
with atomic publishing; that trade-off is accepted and recorded here rather than
left implicit.

## Integration Mode — Confirmed Modular (crate-per-protocol)

Verified on disk: `crates/*` (5 crates) plus a root facade package that
re-exports and coordinates. Chosen for publishability, dependency isolation and
extendability, per §Consequences. The cost is additional manifest overhead, which
is accepted.

Auth insertion points (`auth.rs` in `tizen`, `webos`, `roku`) are trait boundaries
naming the REA Phase 2 work. Real implementations must not be inserted until REA
output is validated on hardware — see `01-playbook.md` §10 and `03-matrix.md`.
`discovery` and `power` have no `auth.rs`: those protocols are unauthenticated and
the file would have been an empty abstraction.
