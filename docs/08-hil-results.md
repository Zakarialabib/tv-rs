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
| 2026-10-24 | Samsung | tv-rs-mdc | not run | not run | not run | not run | not run | not run | not run | not run | not run | UNVERIFIED (no hardware run yet) |

Leave a cell **not run** rather than guessing. A row with gaps is more useful than a
row with assumptions.
