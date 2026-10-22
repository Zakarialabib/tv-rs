# Capability-honesty table — consolidation record (internal)

Not linked from the README or docs index on purpose. Recorded so nobody looks
for the duplicated copies again.

## What it is

One table, one home: which backend honestly advertises which LFD surface
(`PanelLock`, `Orientation`, `NoSignalStatus`; behind `tv-rs-core`'s
`supporting` feature, `LampOrPanelHours`/`ScreenCapture`) and which return
`None`. The governing sentence: **an absent accessor means the protocol cannot
do it.**

## Single home

`docs/03-matrix.md`, § **"Capability truth"** (anchor `#capability-truth`).
The measured counts and the crates.io registry table in the same file are
separate topics and stay where they are.

## What was consolidated

| Former duplicate                                                 | Disposition                                                               |
| ---------------------------------------------------------------- | ------------------------------------------------------------------------- |
| `crates/tv-rs/src/handle.rs` doc comment (facade accessor prose) | `merge-duplicate` — prose now states the rule and links the matrix        |
| `INTEGRATION.md` §2 "Commercial panels"                          | `merge-duplicate` — how-to sample kept, backend table replaced by pointer |
| `README.md` Capabilities row                                     | `merge-duplicate` — one-line summary + pointer                            |
| `03-matrix.md` facade paragraph                                  | `merge-duplicate` — superseded by the §Capability truth table             |
| Resolved "consumer channels never opt out" discussions           | `obsolete-history` — landed in 0.3.0; recorded in CHANGELOG, not restated |
| `docs/internal/lfd-surfaces-handoff.md`                          | superseded card, deleted                                                  |

`keep-exact`: the rule sentence stays verbatim where it is load-bearing —
03-matrix §Capability truth, `crates/mdc/README.md`, and `INTEGRATION.md` §2
(one echo each, phrased as the rule, never as a second table).

## Source pointers

- Accessor set: `crates/tv-rs/src/handle.rs` (`as_panel_lock`, `as_orientation`,
  `as_panel_health`; behind `supporting`, `as_panel_hours`/`as_screen_capturer`).
- Capability variants: `crates/core/src/capability.rs` (15-variant enum).
- MDC row truth: `crates/mdc/src/lib.rs` advertisement + `types.rs` commands.
- Consumer-channel rows (`None`): the facade's Tizen/webOS/Roku clients in
  `crates/tv-rs/src/handle.rs`.
- Usage rule for adopters: `docs/07-adoption.md` rule 2.
