# tv-rs-ir

> **Deprecated in 0.8.0 — not published.** This crate is a `publish = false` workspace
> shim that re-exports [`tv-rs-drivers`](https://crates.io/crates/tv-rs-drivers) with
> the `ir` feature. The code described below lives in `crates/drivers/src/ir/`.
> New code should depend on `tv-rs-drivers`, not on this crate.


Infrared emitter and remote control protocol abstractions for legacy consumer panels and displays.

## Capabilities

- Formats standard NEC, RC-5, and Sony SIRC infrared pulse trains.
- Dispatches commands through connected USB/serial IR blasters or GPIO hardware.
- Implements `tv-rs-core::KeyInput` and `PowerControl` traits.
