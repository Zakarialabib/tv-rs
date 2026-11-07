# tv-rs-companion

Mobile companion core library for smart TV control, discovery gating, QR+SAS pairing ceremonies, and host communication.

## Features

- **`no_std` + `alloc` support**: Designed to embed into mobile and embedded runtimes without pulling standard OS threads or tokio.
- **UniFFI Bridge**: Clean Swift and Kotlin bindings for iOS and Android companion apps.
- **Pairing Ceremonies**: QR payload generation (`tvrs-companion://`) and SAS (Short Authentication String) calculation for mutual verification.
- **Host Control**: Communicates with `tv-rs-hostd` for remote keys, power state, and input control.
- **DeviceHandle Implementation**: Fully implements `tv-rs-core::DeviceHandle` and passes the shared conformance test suite.

## Usage

```rust,no_run
use tv_rs_companion::CompanionClient;

let client = CompanionClient::new();
// Pair via QR/SAS ceremony and dispatch remote commands
```
