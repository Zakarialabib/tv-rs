# tv-rs-local

Local display probing, topology management, and zone mapping for Rust: connector,
EDID identity, physical size, TV likelihood, and semantic venue zone mapping
(entrance, checkout, kitchen, aisle, etc.).

Part of the [`tv-rs`](https://github.com/Zakarialabib/tv-rs) workspace, but usable
on its own. No Tauri, no database, no TV control, no network.

## Install

`tv-rs-local` is on crates.io at `0.2.0` (the workspace's published set moves
together):

```toml
[dependencies]
tv-rs-local = "0.2"
```

Working inside this workspace, use the sibling path:

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

### Display Topology & Zone Mapping

Map screens to physical operational zones so a display is identified semantically
as **"the entrance display in the restaurant"** rather than just `192.168.1.45` or `HDMI-1`:

```rust
use tv_rs_local::{DisplayTopology, MappedDisplay, Zone};

let mut topology = DisplayTopology::new();

// Map an entrance display
topology.map_display(
    MappedDisplay::new(
        "entrance_tv",
        Zone::entrance(),
        "the entrance display in the restaurant",
    )
    .with_ip("192.168.1.45")
    .with_system_name("HDMI-1"),
);

// Map a kitchen KDS screen
topology.map_display(
    MappedDisplay::new(
        "kitchen_kds",
        Zone::kitchen(),
        "kitchen order display",
    )
    .with_ip("192.168.1.46"),
);

// Map a store aisle display
topology.map_display(
    MappedDisplay::new(
        "aisle3_screen",
        Zone::aisle("3"),
        "aisle 3 promotion board",
    ),
);

// Retrieve semantic descriptions:
let labels = topology.semantic_descriptions();
assert_eq!(
    labels[0],
    "the entrance display in the restaurant (192.168.1.45 / HDMI-1)"
);
```

### EDID

The parser is platform-independent: bytes from any source in, the base block out.

```rust
use tv_rs_local::{display_edids, parse_edid};

for display in display_edids() {
    let edid = &display.edid;
    println!(
        "{:?} {} {} {:?}x{:?} cm",
        display.system_name,
        edid.pnp_id(), // "SAM0F99"
        edid.model(),  // product-name descriptor, else the product code
        edid.physical_width_cm,
        edid.physical_height_cm,
    );
    if let Some(timing) = edid.preferred_timing {
        println!("preferred {}x{} @ {:?} Hz", timing.h_active, timing.v_active, timing.refresh_hz());
    }
}

// Bytes from elsewhere (a file, a capture card, a network report):
assert!(parse_edid(&[0u8; 8]).is_err());
```

`Edid` carries the PNP manufacturer id, product code, numeric serial and serial
descriptor, week/year, version/revision, digital input, size in cm (`None` when
the panel only gives an aspect ratio), the product-name descriptor, the preferred
`DetailedTiming`, the extension count, and `checksum_valid` — a bad checksum is
reported, not rejected. Extension blocks (CTA-861, DisplayID) are counted, not
decoded. Field offsets follow VESA E-EDID 1.4 §3; the tests parse two real EDIDs
(see `tests/fixtures/README.md`). `display_edids()` is blocking, like the probe.

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
| Windows       | WMI (`root\WMI`) for EDID; `EnumDisplayDevicesW` / `EnumDisplaySettingsW` for connector and mode; each active monitor's registry EDID (`Enum\DISPLAY\<id>\<instance>\Device Parameters\EDID`, found through its device interface path) for `display_edids()` and when WMI has no record |
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

MIT. See [LICENSE](../../LICENSE).
