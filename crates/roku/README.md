# tv-rs-roku

> **Deprecated in 0.8.0 — not published.** This crate is a `publish = false` workspace
> shim that re-exports [`tv-rs-drivers`](https://crates.io/crates/tv-rs-drivers) with
> the `roku` feature. The code described below lives in `crates/drivers/src/roku/`.
> New code should depend on `tv-rs-drivers`, not on this crate.


Roku External Control Protocol (ECP) client over HTTP REST (port 8060).

## Supported Operations

- **Device Info**: Model, serial, MAC addresses, network configuration.
- **Power & Mode**: PowerOn, PowerOff, standby queries.
- **Key Press**: Standard navigation keys, media transport keys, home, and volume.
- **App Management**: List installed channels and launch apps with custom parameters.
- **Active App Query**: Fast foreground app status via `/query/active-app`.
