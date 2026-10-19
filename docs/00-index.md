# tv-rs — Documentation Index

Start here. The numbered files are the **operational set** — read them in order.
The unnumbered files are **durable references** that change rarely.

## Operational set

| File | Purpose |
|---|---|
| [`01-playbook.md`](01-playbook.md) | The TV Control & Communications Playbook, §1–10: discovery, power, Tizen, webOS, Roku, ADB, DLNA, Cast/AirPlay, and §10 the reverse-engineering gaps. Source material for the whole workspace, plus §11 on the media & signage layer. |
| [`02-naming-conventions.md`](02-naming-conventions.md) | tv-rs specifics: the module inventory per crate, the deliberate deviations from the standard's vocabulary, and the facade re-export naming. Universal naming rules are in the standard §6. |
| [`03-external-refs.md`](03-external-refs.md) | External SDK and protocol docs, prior-art crates, the REA reverse-engineering tooling, and the research findings with what we did about each. |
| [`03-matrix.md`](03-matrix.md) | **State matrix** — per-component Done / Left / Next, verified test counts, publish readiness, the Phase 2 & 3 roadmap, and the remaining gaps. The authoritative status document. |
| [`05-integration-prompts.md`](05-integration-prompts.md) | **Consumer guide** — nine self-contained prompts for integrating the crates into a host app (discovery, wake, per-vendor control, media server, control API, automation, hardware tests), each naming the exact APIs and the definition of done. |
| [`04-rust-package-standard.md`](04-rust-package-standard.md) | **The reusable engineering standard.** Repository shape, facade pattern, crate-root and error contracts, testability, naming, docs structure, dependency policy, CI gates, publishing. Project-agnostic and duplicated into `ezviz-openapi-rs`. |

## Durable references

| File | Purpose |
|---|---|
| [`ADR-001-crate-publication.md`](ADR-001-crate-publication.md) | Why the workspace is six publishable crates, the license and CI decisions, and the release blocker. |
| [`glossary.md`](glossary.md) | Terms: facade, protocol crate, aggregate client, SSDP/SSAP/ECP/DIAL, Phase 1–3, Gaps A–E, license rules. |
| [`skeleton-map.md`](skeleton-map.md) | Compact Playbook § → crate mapping with Phase 1/2/3 assignment. |

## Numbering

This repository uses two `03-` prefixes: `03-external-refs.md` (external
references) and `03-matrix.md` (state matrix). The standard's §7 slot table is
the general rule; this file is the fact on disk. The standard itself sits at
`04-` here rather than `00-`, because the index occupies `00-`.

## Outside `docs/`

- Root [`README.md`](../README.md) — what the project is, crate table, usage, build and test commands, publishing order.
- Crate-level rustdoc — each crate root carries a doc block and at least one doctest (`cargo doc --workspace --open`).

## Keeping these current

`03-matrix.md` is the file most likely to drift, because it makes claims about
code. When code changes, verify its claims rather than trusting them: the test
counts and status columns have been wrong before. Next most likely is
`02-naming-conventions.md`, because its module inventory is a factual claim about
which files exist — re-check it against `*/src/` rather than by eye.

The standard (`04-rust-package-standard.md`) is duplicated into
`ezviz-openapi-rs` as `00-rust-package-standard.md`. It has a single source of
truth here; after editing it, copy it across so the two repositories do not
diverge.
