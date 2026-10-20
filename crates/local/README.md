# tv-rs-local

Local display probing for Rust: connector, EDID identity, mode, physical size and
TV likelihood for the screen this process is running on.

Part of the [`tv-rs`](https://github.com/Zakarialabib/tv-rs) workspace, but usable
on its own. No Tauri, no database, no TV control, no network.

## Install

Once published:

```toml
[dependencies]
tv-rs-local = "0.1"
```

Until then, use the sibling path:

```toml
[dependencies]
tv-rs-local = { path = "../tv-rs/crates/local" }
```

## Use

```rust
use tv_rs_local::{Attachment, DisplayProbe, ProbeInfo, SystemProbe, fallback};

let probes: Vec<ProbeInfo> = SystemProbe.probe();
for probe in &probes {
    println!(
        "{:?}: {:?} {}x{} TV={} ({:.0}%)",
        probe.system_name,
        probe.attachment,
        probe.width,
        probe.height,
        probe.is_tv,
        probe.tv_confidence * 100.0,
    );
}

// No native probe on this platform: keep the mode the host reported, but do
// not invent a TV claim.
let conservative = fallback(Some("HDMI-1"), 1920, 1080, Some(60));
assert_eq!(conservative.attachment, Attachment::Hdmi);
assert!(!conservative.is_tv);
```

From async code, use `probe_displays_async()`: the Windows WMI/COM path is
blocking and must not run on the runtime thread.

## What it returns

`ProbeInfo` is one attached display:

| Field                                     | Meaning                                                                |
| ----------------------------------------- | ---------------------------------------------------------------------- |
| `system_name`                             | Connector key (`\\.\DISPLAY1`, `HDMI-A-1`); not an app display id      |
| `width`, `height`                         | Current mode in pixels                                                 |
| `refresh_rate`                            | Current mode refresh in Hz                                             |
| `vendor`, `model`, `serial`               | EDID identity where available                                          |
| `attachment`                              | `Internal`, `Hdmi`, `DisplayPort`, `Dvi`, `Vga`, `Wireless`, `Unknown` |
| `physical_width_cm`, `physical_height_cm` | EDID/panel size                                                        |
| `brightness`                              | Platform brightness, where exposed                                     |
| `is_tv`, `tv_confidence`, `evidence`      | Conservative classification and its reasons                            |

`Unknown` means the probe could not establish the connector; it is not a claim
that the display is not a TV. Callers should fall back to `fallback()` and show
"Unknown" rather than guessing.

## Platforms

| Platform      | Implementation                                                                                   |
| ------------- | ------------------------------------------------------------------------------------------------ |
| Windows       | WMI (`root\WMI`) for EDID; `EnumDisplayDevicesW` / `EnumDisplaySettingsW` for connector and mode |
| Linux         | `/sys/class/drm` connectors, mode list and EDID                                                  |
| macOS / other | No native probe yet; returns an empty list so the caller can use its own monitor data            |

`unsafe` is denied at the crate root and allowed only inside the Windows module.

## Checks

```text
cargo test -p tv-rs-local
cargo clippy -p tv-rs-local --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc -p tv-rs-local --no-deps
```

## License

MIT. See [LICENSE](LICENSE).
