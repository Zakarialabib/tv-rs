# tv-rs-discovery

Universal display discovery, device fingerprinting, and persistent device registry:
finds televisions, digital signage panels, streaming boxes, and media receivers across LAN and peripheral buses.

```rust,no_run
use std::time::Duration;
use tv_rs_discovery::{UniversalScanner, DeviceRegistry};

# async fn run() -> tv_rs_discovery::Result<()> {
let mut scanner = UniversalScanner::new().with_timeout(Duration::from_secs(2));

// Run multi-protocol discovery and save devices into registry
let saved_devices = scanner.scan_and_save().await;

for dev in saved_devices {
    println!("Found display: {} ({:?})", dev.friendly_name, dev.kind);
}
# Ok(())
# }
```

# Supported Protocols & Scanning

| Discovery Component | Protocol / Target | Identifies |
| ------------------- | ----------------- | ---------- |
| `SsdpScanner` | UDP 1900 multicast M-SEARCH | UPnP devices, Samsung Tizen, LG webOS |
| `MdnsScanner` | UDP 5353 mDNS / Bonjour | Signage players, Smart TVs |
| `AdbScanner` | TCP 5555 ADB daemon | Android TV boxes, Fire TVs, Amlogic devices |
| `BluetoothScanner` | BLE Advertising & EIR frames | BLE remotes, Fire TV sticks, signage beacons |
| `CastScanner` | `_googlecast._tcp.local` mDNS | Google Chromecasts, Cast receivers |
| `LocalHttpDisplayScanner` | TCP 80, 8060, 3000, 1515, 8080, 8443 | Roku ECP, LG webOS SSAP, Samsung MDC, SICP panels |
| `CecTopologyScanner` | HDMI-CEC logical/physical addresses | Anynet+, SimpLink, Bravia Sync, CEC Vendor IDs |
| `DeviceRegistry` | Persistent JSON registry | Saved devices, tokens, first/last seen timestamps |

# Innovation: Device Fingerprinting

The `DeviceFingerprint` aggregates signatures across all protocols:
- **MAC pattern / OUI**
- **mDNS host name**
- **SSDP Server header**
- **Open TCP ports**
- **HDMI-CEC Vendor ID**
- **Bluetooth name**
- **ADB model string**

Fingerprint identification classifies displays into:
- **TV Brands:** Samsung Tizen, LG webOS, Sony Bravia, Roku TV
- **Android Boxes:** NVIDIA SHIELD, Mi Box, generic Android TV
- **Amlogic Devices:** Reference boards, OTT set-top boxes
- **Fire TV Devices:** Amazon Fire TV sticks, cubes, Fire Edition TVs
- **Chromecast-like Devices:** Google Chromecast, Cast receivers
- **Commercial Panels:** Samsung MDC, SICP, LG webOS Signage

# Rules it holds to

- **Silence is an error, not an empty list.** `DiscoveryError::SilentProbe` means nothing answered at all, naming both permission/network reachability and silent hosts.
- **Dedup & Merging.** Multiple protocol responses from the same physical display are merged into a single `DiscoveredDevice` / `SavedDevice`.
- **Silence on missing endpoints.** A broken or unresponsive HTTP probe fails gracefully without interrupting remaining scans.

# Development

- **Tests:** `cargo test -p tv-rs-discovery`.
- **Formatting:** `cargo fmt --all --check` must be clean.
