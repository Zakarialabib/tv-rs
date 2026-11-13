# Changing this workspace: breaking, improving, or a better way

**Read this before your first PR.** It is written for someone who has just cloned the
repo and is wondering whether their idea is a small fix or a release-blocking change.
Every claim below is checked against the code; when code and docs disagree, the code is
right and the page is a bug.

## The one question

> Does a caller's existing code still compile, and still behave the same?

The answer puts the change in one of three classes. The class decides the version bump,
the CHANGELOG section, and whether a migration note is required — so classify before you
write, not after.

| Class | The answer to the question | Version bump (at `0.x`) | CHANGELOG section |
| --- | --- | --- | --- |
| **Breaking** | no — it stops compiling, or behaves differently | minor (`0.8` → `0.9`) | `Changed` or `Removed` |
| **Improvement** | yes — same surface, better inside | patch or minor | `Fixed` / `Added` / `Changed` |
| **A better way** | it *should* change the surface | minor, plus a migration note | `Changed` / `Removed` |

## Class 1 — Breaking

A change that makes a downstream `match` non-exhaustive, a build fail, or a default
silently change meaning.

- **Adding, removing or renaming a variant of `TvError` or `Remedy`**
  (`crates/core/src/error.rs`). Downstream retry code matches exhaustively over
  `Remedy`; a new variant is a compile error for them, not a new feature for them.
  Adding a variant, its `Remedy`, and the exhaustiveness test case belongs in one PR.
- **Removing or repurposing a `TvError` variant.** The 90-day rule in
  [`CONTRIBUTING.md`](../CONTRIBUTING.md#versioning-policy) exists because `Remedy`
  mapping lives in downstream retry loops, not in this repo.
- **Removing or renaming a public item**, or reordering a method's arguments.
- **Changing a default that alters behaviour** — a port order, a timeout, a confidence
  threshold. A caller who never passed the argument is now on a different code path.
- **Raising `rust-version`.** CI builds the declared MSRV (1.89) as its own job; a
  bump is a breaking change for everyone on an older toolchain.
- **Changing the dependency graph of a published crate.** `path` dependencies are
  stripped at publish time, so a published crate may only name crates that exist on
  crates.io at the version it pins. The 0.8.0 release stopped at crate #7
  (`tv-rs-sim`) for exactly this: it named a deprecated `publish = false` shim that
  only existed on the registry at 0.7.1. See
  [`01-architecture.md` §Publishing](01-architecture.md#publishing).

## Class 2 — Improvement

Same public surface, better inside. No caller has to change anything.

- a parser bug fix, a clearer error reason, a faster path, more test coverage;
- a new capability on a backend — an absent capability already returns a typed
  `Unsupported` error, so adding one changes no signature;
- a new defaulted method on a trait, a new re-export behind an existing feature, a new
 field on a struct that is `#[non_exhaustive]` (the ones in `crates/local/src/edid.rs`
 and `crates/local/src/lib.rs`).

Still needs a CHANGELOG line. "No behaviour change" is a claim the diff has to earn.

## Class 3 — A better way of handling things

The surface itself is wrong: a name that misleads, a crate that should be two, a
dependency direction that should flip. The rule is **deprecate fully** — no interim
state, no half-migrated API. Either the old thing is gone, or the change has not started.

Two shapes are allowed:

- **Remove it.** The old name disappears; the CHANGELOG's `Removed` section and the
  migration note are the only trace. This is the default.
- **Keep the old name compiling, but never publish it.** When in-repo code and tests
  still name an old crate, that crate becomes a `publish = false` shim that
  glob-re-exports the new home and carries a `#[deprecated]` note. A shim is
  workspace-internal: **a published crate must never depend on one**, because `path`
  is stripped at publish time and the shim does not exist on the registry. Shims are
  deleted when the last in-repo reference migrates.

The worked example is the 0.8.0 consolidation: ten vendor crates became one
`tv-rs-drivers` behind feature flags, and the old names stayed as `publish = false`
shims that glob-re-export the new modules. That shape was chosen deliberately — the
migration path is "change the dependency, keep the imports" — and it is recorded in
[`01-architecture.md` §Publishing](01-architecture.md#publishing), not left for a
newcomer to rediscover. It is also the trap that stopped the 0.8.0 release: `tv-rs-sim`
and the facade still named two of those shims, so cargo could not resolve them.

When the better way is an **architecture** decision, record the *why* in
[`docs/adr/`](adr/). An ADR is a verdict plus the investigation that produced it
(`0001-lg-serial-claim.md` is the template: Verdict, Claim, Investigation, Conclusion).
When it is a **user-visible** change, the *what* goes in the CHANGELOG under `Changed`
or `Removed`, with a migration note in the docs a caller will actually read.

## Where each class is recorded

| Class | Record it in |
| --- | --- |
| Breaking | `CHANGELOG.md` under `Changed`/`Removed`, a migration note in the docs, and a minor bump via `cargo xtask publish --version` |
| Improvement | `CHANGELOG.md` under `Fixed`/`Added`/`Changed`; `docs/03-matrix.md` when a status row changes |
| Better way | `docs/adr/NNNN-*.md` for the decision, `CHANGELOG.md` for the change, `06-plan.md` if it is still open work |

`docs/06-plan.md` is the only plan and it never grows a history: an item leaves the page
when it lands, and landed work lives in the CHANGELOG.

## Traps that turn a small change into a breaking one

- **Feature unification hides breakage.** `cargo test --workspace` builds the union of
  every feature, so a crate can be broken on its own and green in the workspace. CI's
  `per-crate features` job exists for exactly this; run `cargo check -p <crate>` before
  you trust a green workspace.
- **A Windows-local clippy pass is not Linux CI.** Enum-size lints measure differently
  per target. Fix the cause (box the variant); never widen an allow list to go green
  locally.
- **`publish = false` members are not published.** `tv-rs-cli`, `tv-rs-androidtv`, the
  ten deprecated vendor shims, and `tests/e2e_suite` are workspace-only. A published
  crate that depends on one of them cannot ship.
- **Hardware-in-the-loop tests are opt-in.** `--features hil` plus the `TV_RS_HIL_*`
  environment variables; they must never run in CI, where no television exists.
- **Docs are claims.** `docs/03-matrix.md` numbers carry the date they were measured.
  If you change behaviour, change the page in the same PR — a stale page is a defect,
  not a follow-up.

## Before you open the PR

1. `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
   `cargo test --workspace`, `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`.
2. `cargo check -p <your crate>` — the workspace run is not proof.
3. Classify the change (this page) and write the CHANGELOG line in the same commit.
4. If the change is a better way, open the ADR *before* the refactor, not after.
