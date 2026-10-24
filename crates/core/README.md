# tv-rs-core

The device model shared by every `tv-rs` backend: capability traits, an erased
device handle, command ids, confidence-tagged observed state and the routing
error taxonomy. It is *vocabulary, not protocol* — it knows a television has
capabilities, a state that cannot always be observed, and failures that mean
different things, and nothing about ports, handshakes or XML.

```no_run
use std::sync::Arc;
use tv_rs_core::{CommandId, DeviceHandle, Key, KeyInput};

async fn press(handle: &Arc<dyn DeviceHandle>, key: Key) -> Result<(), tv_rs_core::TvError> {
    let input = handle
        .as_key()
        .ok_or(tv_rs_core::TvError::Unsupported(tv_rs_core::Capability::Key))?;
    input.key_press(key, CommandId::new()).await
}
```

# Three rules this crate exists to enforce

1. **No generic leaks.** A fleet holds `Arc<dyn DeviceHandle>`; a caller asks
   `as_power()` and treats `None` as "this device cannot do that", which is the
   same fact as [`TvError::Unsupported`] but knowable before the attempt.
2. **State carries confidence.** A TV that stopped answering is not off — it is
   [`Confidence::Unknown`], and saying so is more useful than a boolean.
3. **Errors route.** [`TvError`] is a decision table: re-pair, reconnect, back
   off, or stop.

# What it deliberately does not do

- It does not speak a protocol, so it cannot time out, parse a frame or hold a
  socket. A trait is a *capability contract*, not an implementation.
- It does not carry a clock you do not own. `ObservedField` timestamps are
  **monotonic** (`Instant`): there is no epoch to convert, and no `tokio`
  dependency, so the core builds everywhere.
- It does not choose a flavour of TLS. Cryptography is a *policy* concern owned
  by the transport crate — this crate is protocol-free.

## Features

| Feature      | What it enables                                                  | Off by default |
| ------------ | ---------------------------------------------------------------- | -------------- |
| `conformance` | Protocol-conformance assertions for CI/hardware                 | no             |
| `hil`        | Hardware-in-the-loop stubs (gated `cfg` surfaces)                | no             |
| `supporting` | Panel hours, still-frame capture, app-launch surfaces            | yes            |
| `quirks`     | Known-device fix-ups (`[quirks]` table + `QuirkId`)              | no             |
| `serde`      | **Optional JSON (de)serialization** of the device model          | **yes**        |

### The `serde` feature

The core device model is **not** `Serialize` by default. Enable it with

```toml
tv_rs_core = { version = "0.3", features = ["serde"] }
```

The wire contract is the contract with the companion's `types.ts`:

- every capability, key and input serialises as its snake_case label (`"power_state"`,
  `"digit_7"`, `"hdmi2"`, `"tv"`);
- `Key::Digit(n)` becomes `"digit_n"` (not `"digit": n`);
- `ObservedField` becomes
  `{"age_ms": u64, "confidence": "high"|"inferred"|"unknown", "value": T}`. There is
  no `verified_at_ms`: the timestamp is a monotonic `Instant`, which has no epoch, so
  only the age is derivable;
- every other data type derives `Serialize`/`Deserialize` with
  `rename_all = "snake_case"` (enums `samsung`, `webos`, `roku`, `power_state`…).

The contract is pinned by tests rather than by this page: `cargo test -p tv-rs-core
--features serde` asserts every capability's JSON equals its `as_str()` label, so the
derive and the label cannot drift.

No `serde` dependency lands unless the feature is on, so a cloud agent can carry
the core crate with zero serialisation cost.

## Development

- **Tests:** `cargo test --workspace`.
- **No wrap:** `cargo fmt --all --check` must be clean.
- **No panics in the public API:** errors are `TvError`, never `unwrap()`/`panic!`
  in consumer-facing code (the `hil` crate owns the hardware probes).
