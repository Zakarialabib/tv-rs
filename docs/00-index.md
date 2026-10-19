# Documentation

Nine crates, one facade, one idea: speak each TV's own protocol and never hide a
failure behind a panic. These pages are the long version of that.

## Read in this order

| # | Page | What it answers |
|---|---|---|
| 0 | [`glossary.md`](glossary.md) | **The map.** Every term, and the page or file that explains it. Start here if you only have five minutes. |
| 1 | [`01-playbook.md`](01-playbook.md) | The protocol facts: ports, handshakes, discovery, media, and what each vendor exposes. |
| 2 | [`02-naming-conventions.md`](02-naming-conventions.md) | Where a file, trait, struct or constant belongs. |
| 3 | [`03-matrix.md`](03-matrix.md) | State: what is implemented, what is deferred, verified test counts, remaining gaps. |
| 4 | [`03-external-refs.md`](03-external-refs.md) | Protocol references, prior art, and the crates this workspace learned from. |
| 5 | [`04-rust-package-standard.md`](04-rust-package-standard.md) | The engineering standard the whole workspace follows — reusable outside this repo. |
| 6 | [`ADR-001-crate-publication.md`](ADR-001-crate-publication.md) | Why eight crates, how a release is ordered, and the CI that guards it. |
| — | [`skeleton-map.md`](skeleton-map.md) | Which playbook section became which crate. |

## Conventions used here

- **Claims about code are checked against code.** If a page says a function
  exists, it exists in `src/`. When the two disagree, the code is right and the
  page is a bug.
- **Deferred is a feature.** A capability that is not implemented returns a typed
  `Unsupported` error with a reason. No page promises something the code refuses
  to do.
- **Numbers are dated.** Test counts and file inventories in `03-matrix.md` carry
  the date they were measured; they rot, the source does not.

## Where the code lives

| Path | Contents |
|---|---|
| `src/` | facade: discovery orchestration, vendor identification, `TvClient` |
| `discovery/`, `power/`, `tizen/`, `webos/`, `roku/`, `transport/`, `media/` | protocol and support crates, one per concern |
| `cli/` | the reference consumer (`publish = false`) |
| `tests/` | hardware-in-the-loop suite, compiled out by default |
| `scripts/` | ordered release |
