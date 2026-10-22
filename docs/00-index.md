# Documentation

Fourteen crates, one facade, one idea: speak each TV's own protocol and never hide a
failure behind a panic. These pages are the long version of that.

## Read in this order

| # | Page | What it answers |
|---|---|---|
| 0 | [`glossary.md`](glossary.md) | **The map.** Every term, and the page or file that explains it. Start here if you only have five minutes. |
| 1 | [`01-playbook.md`](01-playbook.md) | The protocol facts: ports, handshakes, discovery, media, and what each vendor exposes. |
| 2 | [`architecture.md`](architecture.md) | The layers, the dependency rule, the module inventory per crate, the auth rule, and the playbook → crate map. |
| 3 | [`03-matrix.md`](03-matrix.md) | State: what is implemented, what is deferred, verified test counts, remaining gaps. |
| 4 | [`02-external-refs.md`](02-external-refs.md) | Protocol references, prior art, and the crates this workspace learned from. |
| 5 | [`04-rust-package-standard.md`](04-rust-package-standard.md) | The engineering standard the whole workspace follows — reusable outside this repo. |
| 6 | [`06-plan.md`](06-plan.md) | **The live plan.** Only open work: the `0.3.0` publish, Sony, native CEC, DDC/CI, hardware, media caching, the next release. |
| — | [`../INTEGRATION.md`](../INTEGRATION.md) | **Putting it in an application**: discovery, TLS pinning, quirks, a fleet, media, CEC, the pre-ship checklist. |
| — | [`07-adoption.md`](07-adoption.md) | The contract for adopting this workspace inside a signage application (U0–U4): what the app owns, what the crate owns. |
| — | [`05-roadmap.md`](05-roadmap.md) | Archive: the P0–P9 proposals and how each one actually resolved. |
| — | [`hil-results.md`](hil-results.md) | What has actually run against a physical TV. Today: nothing. |
| — | [`adr/`](adr/) | [001 — crate publication](adr/001-crate-publication.md), [002 — workspace layout](adr/002-workspace-layout.md). |

## Conventions used here

- **Claims about code are checked against code.** If a page says a function
  exists, it exists in `crates/`. When the two disagree, the code is right and the
  page is a bug.
- **Deferred is a feature.** A capability that is not implemented returns a typed
  `Unsupported` error with a reason. No page promises something the code refuses
  to do.
- **Numbers are dated.** Test counts and file inventories in `03-matrix.md` carry
  the date they were measured; they rot, the source does not.

## Where the code lives

| Path | Contents |
|---|---|
| `crates/tv-rs` | facade: discovery orchestration, vendor identification, `TvClient` |
| `crates/{discovery,power,tizen,webos,roku}` | one wire protocol each |
| `crates/{transport,media}` | support: TLS/frame reader; `206`/HLS server |
| `crates/cli` | the reference consumer (`publish = false`) |
| `xtask` | workspace automation: `cargo xtask publish` |
| `docs/` | these pages |
| `docs/internal/` | notes kept out of the public set (not linked from here) |