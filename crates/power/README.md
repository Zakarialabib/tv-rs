# tv-rs-power

> **Deprecated in 0.8.0 — not published.** This crate is a `publish = false` workspace
> shim that re-exports [`tv-rs-drivers`](https://crates.io/crates/tv-rs-drivers) with
> the `power` feature. The code described below lives in `crates/drivers/src/power/`.
> New code should depend on `tv-rs-drivers`, not on this crate.


Wake-on-LAN (WoL) packet transmission and multi-stage TCP/HTTP power probing.

## Features

- **Magic Packet Generator**: Dispatches standard UDP broadcast Wake-on-LAN packets to target MAC addresses.
- **Three-Tier Power Probe**: Probes target IPs over well-known TV ports to classify state as `On`, `Standby`, or `Off` with associated confidence levels.
