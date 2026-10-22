# tv-rs-mdc

Samsung Multiple Display Control (MDC) over TCP 1515 or RS-232.

```toml
[dependencies]
tv-rs-mdc = "0.3"
```

`tv-rs-mdc` is new: it publishes with the `0.3.0` release and is not on the registry
before that. Everything else in this workspace's published set is at `0.2.0` today.

## What it does

- Encodes and checksums the documented frame:
  `0xAA | command | display id | length | data | checksum`
- Addresses one panel or a video wall, sharing a single connection between
  display ids.
- Runs over **two carriers**: TCP 1515 by default, or an RS-232 line behind the
  `serial` feature. One frame codec, one checksum, one reply parser — a wired wall is
  the same protocol on a different wire.
- Reads and writes power, input, volume, mute and brightness.
- Reads the aggregated status command (`0x00`) with typed power, volume, mute,
  input and aspect fields.
- Speaks the LFD subset a signage panel actually needs: panel key lock (`0x5F`),
  content orientation (`0xC8`/`0x82`), signal state and panel health (`0x0D` —
  lamp fault, input-source error, panel temperature, fan), and panel on-time
  (`0x83` behind the `supporting` feature).
- Exposes `MdcDevice`, a `DeviceHandle` adapter covering power, input, volume,
  **brightness**, **reboot**, the **LFD subset above** and a
  **control-channel probe** (`ping()` returns the round trip of a status query),
  plus identity as `DeviceOs::SamsungMdc`.

## What it refuses

- It does not claim screen capture. No MDC command captures the panel's
  picture, so `as_screen_capturer()` returns `None` in every configuration and
  `Capability::ScreenCapture` is never advertised. A caller can therefore tell
  "this backend cannot do that" from "this reading is currently unknown" — the
  two are different, and only the second is worth retrying.
- No aggregate-`Telemetry` provider. The `0x0D` status is exposed as
  `PanelHealth` (`as_panel_health()`), not behind `Capability::Telemetry`,
  because MDC performs no structured telemetry reads (no signal-strength
  history, no uptime, no network diagnostics).
- It does not invent a wake path. Samsung documents Wake-on-LAN for a display
  that is off; a TCP MDC frame is not a wake transport, so `wake()` returns
  `Unsupported(Wake)`.
- It does not silently clamp volumes or brightness. A value above 100 is an
  error.
- It does not retry a mutating command after a timeout: the command may have
  been executed and only the reply lost.

## TV firmware and the carriage return

The MDC document defines the bare frame. Some TV firmware also expects `0x0D`
after it. The transport defaults to off, matching commercial displays:

```rust
use std::time::Duration;
use tv_rs_mdc::{MdcClient, MdcOptions};

let client = MdcClient::with_options(
    "192.168.1.20",
    MdcOptions {
        timeout: Duration::from_secs(3),
        ..MdcOptions::default()
    }
    .with_line_terminator(true),
);
```

## RS-232 (`serial` feature)

A video wall wired over RS-232 has no discovery, no DHCP and no TLS to argue about.
Turn the feature on and the same client speaks the same frames over the line:

```toml
[dependencies]
tv-rs-mdc = { version = "0.3", features = ["serial"] }
```

```rust
# #[cfg(feature = "serial")]
# async fn run() -> Result<(), tv_rs_mdc::MdcError> {
use tv_rs_mdc::MdcClient;

// /dev/ttyUSB0 on Linux, COM3 on Windows. Samsung LFDs are documented at 9600 8N1.
let wall = MdcClient::serial("/dev/ttyUSB0", 9600);
wall.power_on().await?;
# Ok(())
# }
```

`serial` is off by default, so an agent that only ever talks to networked panels
compiles no serial stack. Opening a device is a local syscall rather than a network
round trip, so there is nothing to time out: a missing device is an immediate
`MdcError::Io`, and the crate's tests drive the request loop over an in-memory channel
to keep the "one codec, two carriers" claim honest.

## Video walls

Clones share the same connection; `display()` changes only the addressed id:

```rust
use tv_rs_mdc::{DisplayId, MdcClient};

# async fn run() -> Result<(), tv_rs_mdc::MdcError> {
let wall = MdcClient::new("192.168.1.20");
for id in 1..=4 {
    let panel = wall.display(DisplayId::new(id)?);
    panel.power_on().await?;
}
# Ok(())
# }
```
