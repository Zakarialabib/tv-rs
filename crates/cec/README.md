# tv-rs-cec

> **Deprecated in 0.8.0 — not published.** This crate is a `publish = false` workspace
> shim that re-exports [`tv-rs-drivers`](https://crates.io/crates/tv-rs-drivers) with
> the `cec` feature. The code described below lives in `crates/drivers/src/cec/`.
> New code should depend on `tv-rs-drivers`, not on this crate.


HDMI-CEC control and platform-native bindings for display power, standby, and active source switching.

## Transports

1. **Subprocess (`cec-client`)**: Default backend. Spawns `cec-client` to command attached CEC adapters (e.g. Pulse-Eight) without C toolchain requirements.
2. **Native FFI (`native`)**: Feature-gated low-latency native implementation wrapping `/dev/cec` and platform APIs with audited FFI boundaries and frame validation.
3. **Measurement (`measurement`)**: Benchmark and latency instrumentation for sub-10ms CEC command round-trips.

## Features

```toml
[dependencies]
tv-rs-cec = { version = "0.6", features = ["client"] }
```
