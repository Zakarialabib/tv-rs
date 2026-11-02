# Hardware verification results

What has actually been run against a physical device. One row per device, added
when someone runs the `hil` suite (`cargo test -p tv-rs --features hil`) or a manual
probe. `cargo xtask hw-test --write-doc` appends a row ([plan item 7](06-plan.md)).

**Current state: nothing has been verified on hardware.** An SSDP `M-SEARCH` from
the development host on 2026-10-19 returned zero non-loopback responders, so no TV
was reachable for a verification pass. Every behavioural claim in this repository is
currently documentation-derived.

## Format

| Date | Vendor | Model | Year | Firmware | Discover | Pair | Key | Launch | Power read | Power off | Wake | Notes |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 2026-10-24 | Samsung | tv-rs-mdc | not run | not run | not run | not run | not run | not run | not run | not run | UNVERIFIED (no hardware run yet) |

## In progress (headless)

- **ATR v2 (plan item 2.4)** — protocol facts verified offline against the
  reference client (`androidtvremote2`); the crate is implemented and unit-tested
  ([`06-plan.md` §2.4](06-plan.md), [`04-androidtv-remote-v2-spike.md`](04-androidtv-remote-v2-spike.md)).
  The 9 ordered probes run with one command —
  `cargo run -p tv-rs-androidtv --example probe -- <host> [pairing-code]` — and print
  ready-made rows for the table above; the run and the verdict row are pending a real
  panel. Pairing is the gate: without the 6-hex code shown on the TV the runner stops
  after the on-screen prompt.

- **Everything else in the `sim` exits** (webOS A2, Tizen A2 + TOFU, DLNA/GENA event
  push, MDC, SICP) is CI-verified against mock servers speaking the recorded wire
  shapes — see [`06-plan.md` §3](06-plan.md). Those rows are *mock*-verified, not
  hardware-verified: this table stays untouched until a real panel answers.

Leave a cell **not run** rather than guessing. A row with gaps is more useful than
a row with assumptions.
