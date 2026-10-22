# tv-rs-mdc

Samsung Multiple Display Control (MDC) over TCP 1515.

```toml
[dependencies]
tv-rs-mdc = "0.3"
```

## What it does

- Encodes and checksums the documented frame:
  `0xAA | command | display id | length | data | checksum`
- Addresses one panel or a video wall, sharing a single TCP socket between
  display ids.
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
