# tv-rs-sicp

> **Deprecated in 0.8.0 — not published.** This crate is a `publish = false` workspace
> shim that re-exports [`tv-rs-drivers`](https://crates.io/crates/tv-rs-drivers) with
> the `sicp` feature. The code described below lives in `crates/drivers/src/sicp/`.
> New code should depend on `tv-rs-drivers`, not on this crate.


Philips Serial Information Control Protocol (SICP) over TCP 5000 and RS-232 serial.

## Features

- Binary frame serialization with length header and XOR checksum.
- Multi-display addressing (`DisplayId`).
- Power status and control (On, Off, Standby).
- Input switching (HDMI1, HDMI2, DisplayPort, VGA, DVI, etc.).
- Volume control (0-100).
- Display temperature and signal status queries.
- Implementation of `tv-rs-core::DeviceHandle`.
