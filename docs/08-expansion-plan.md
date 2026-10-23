# tv-rs Expansion Plan: Ports & New Layers

## Summary

tv-rs becomes the most complete open signage control stack by adding three things it lacks: more display protocols, an edge-host runtime, and a mobile companion core. All of it stays pure Rust, with no desktop UI (the existing Tauri platform covers that) and no database (state lives in files and in the devices themselves).

Today tv-rs covers Samsung (Tizen, MDC), LG webOS, Roku, DLNA, Wake-on-LAN, SSDP and HDMI-CEC. The gaps that matter most for signage:

- **Commercial displays beyond Samsung:** Philips SICP, LG commercial RS-232/IP, Sony Bravia Pro, Sharp/NEC, projectors over PJLink.
- **Consumer platforms:** Android TV / Google TV, Cast, Apple TV, Philips JointSPACE, Hisense VIDAA, Vizio SmartCast.
- **Host hardware:** DDC/CI for monitors, native CEC, IR blasters, GPIO and sensors, display output control.
- **Edge player runtime:** playback, sync across a video wall, proof-of-play, watchdog, offline content cache.
- **Mobile companion:** a shared Rust core for pairing, provisioning and LAN control, exposed to Swift and Kotlin.

The plan adds about 15 crates in four phases. Commercial-display protocols come first because they are small, documented byte protocols and the highest-value gap for signage.

## Port candidates

The best ports are small wire protocols with a public spec and a reference implementation in Python, C# or JS. Port the protocol from the spec and use the reference library only to check behaviour, so its licence doesn't carry over. Port numbers, licences and maintenance status are approximate and were not verified page by page; confirm each against the vendor spec before building.

### Commercial displays and AV (highest value)

| Target | Reference implementation | Wire protocol | Effort | Proposed crate |
| --- | --- | --- | --- | --- |
| Philips signage | [philips-signage-display-sicp](https://github.com/aolde/philips-signage-display-sicp) (C#, also on NuGet) | SICP: binary frames, checksum, TCP 5000 or RS-232 | Low, same shape as MDC | `tv-rs-sicp` |
| LG commercial | [libLGTV\_serial](https://github.com/ehjortberg/libLGTV_serial), [lg-signage](https://www.npmjs.com/package/lg-signage) (npm), [openHAB LG serial binding](https://www.openhab.org/addons/bindings/lgtvserial/) | ASCII commands (`ka 01 01`), RS-232 or TCP 9761 | Low | `tv-rs-lgcmd` |
| Sony Bravia Pro | [Sony Simple IP Control spec](https://pro-bravia.sony.net/remote-display-control/simple-ip-control/), [sonybraviatv-remotecontrol](https://pypi.org/project/sonybraviatv-remotecontrol/) | Fixed 24-byte ASCII frames on TCP 20060, plus a REST/JSON API | Low | `tv-rs-bravia` |
| Sharp/NEC LFD | [nec-pd-sdk](https://pypi.python.org/pypi/nec-pd-sdk) (Python), [external control manual](https://assets.sharpnecdisplays.us/documents/usermanuals/external-control-e988.pdf) | NEC binary protocol, TCP 7142 or RS-232 | Medium, larger command set | `tv-rs-necpd` |
| Projectors | [pypjlink](https://pypi.org/project/pypjlink/), [aiopjlink](https://pypi.org/project/aiopjlink) | PJLink Class 1/2, TCP 4352, ASCII, MD5 auth | Low | `tv-rs-pjlink` |

### Consumer platforms

| Target | Reference implementation | Wire protocol | Effort | Proposed crate |
| --- | --- | --- | --- | --- |
| Android TV / Google TV | [androidtvremote2](https://github.com/tronikos/androidtvremote2) (Python) | Remote protocol v2: protobuf over TLS, pairing on 6467, control on 6466 | Medium | `tv-rs-androidtv` |
| Android devices (sideload, shell) | [adb\_client](https://www.lib.rs/crates/adb_client) already exists in Rust | ADB over TCP 5555 | Wrap, don't port | inside `tv-rs-androidtv`, feature `adb` |
| Chromecast / Cast | [rust\_cast](https://docs.rs/crate/rust_cast) already exists in Rust | Cast v2: protobuf over TLS 8009 | Wrap, don't port | `tv-rs-cast` |
| Apple TV | [pyatv](https://pypi.org/project/pyatv/) (Python) | Companion, AirPlay 2, MRP; HAP-style pairing | High, crypto-heavy | `tv-rs-appletv` (later) |
| Philips consumer | [ha-philipsjs](https://pypi.org/project/ha-philipsjs/) | JointSPACE REST, digest auth on 1926 | Low | `tv-rs-jointspace` |
| Hisense VIDAA | [hisensetv](https://hisensetv.readthedocs.io/) | MQTT on the TV, port 36669 | Low, reuse `rumqttc` | `tv-rs-hisense` |
| Vizio | [pyvizio](https://github.com/raman325/pyvizio); a small [smartcast](https://docs.rs/crate/smartcast/latest) Rust crate exists | SmartCast REST over HTTPS 7345/9000, PIN pairing | Low | `tv-rs-vizio` |

### Hardware around the screen

| Target | Reference implementation | Wire protocol | Effort | Proposed crate |
| --- | --- | --- | --- | --- |
| IR blasters | [python-broadlink](https://docsearch.algolia.com/mcp/docs/repo/mjg59/python-broadlink) | Broadlink UDP, AES-CBC payloads | Medium | `tv-rs-ir` |
| Monitors | [ddc-hi](https://docs.rs/ddc-hi) already exists in Rust | DDC/CI (MCCS VCP codes) over I²C | Wrap | `tv-rs-ddc` |
| HDMI-CEC (native) | [cec-rs](https://lib.rs/crates/cec-rs) bindings to libcec | CEC via libcec | Wrap, replaces the `cec-client` subprocess | upgrade `tv-rs-cec` |
| Wi-Fi provisioning | [Improv](https://improv.hexdocs.pm/readme.html) spec; [improv-wifi](https://docs.rs/improv-wifi) crate | BLE GATT or serial | Low | `tv-rs-provision` |

Also worth mining for behaviour, not code: Home Assistant integrations (quirks per model) and control-system driver stores such as RTI and Crestron, whose help PDFs document many command sets.

## Rust crates to reuse instead of porting

Wrap these behind tv-rs traits rather than rewriting them. Each wrapper is an optional feature, so the default build stays small.

| Need | Crate | Use in tv-rs |
| --- | --- | --- |
| Cast v2 control | [rust\_cast](https://docs.rs/crate/rust_cast) | `tv-rs-cast` backend implementing `DeviceHandle` |
| ADB over TCP | [adb\_client](https://www.lib.rs/crates/adb_client) | sideload and shell on Android-based panels |
| mDNS / DNS-SD | [libmdns](https://lib.rs/crates/libmdns), [simple-mdns](https://docs.rs/simple-mdns) | discover Cast, AirPlay, Android TV; advertise the edge host |
| DDC/CI | [ddc-hi](https://docs.rs/ddc-hi) | brightness, input, power on monitors attached to the host |
| HDMI-CEC | [cec-rs](https://lib.rs/crates/cec-rs) | native CEC, replacing the `cec-client` subprocess |
| Precise time | [statime](https://tweedegolf.nl/en/blog/72/announcing-statime-a-rust-ptp-implementation) (PTP) | frame-accurate sync across a video wall |
| Wi-Fi provisioning | [improv-wifi](https://docs.rs/improv-wifi) | first-boot setup from the phone |
| Playback | gstreamer-rs or libmpv bindings | the edge player's media pipeline |
| Display output | `drm` / KMS crates | mode setting, rotation, blanking on bare Linux |
| GPIO, I²C, sensors | `rppal`, `embedded-hal`, `linux-embedded-hal` | presence sensors, relays, panel temperature |
| BLE | `btleplug` | phone pairing, beacons, provisioning |
| Serial | `tokio-serial` (already used) | RS-232 for SICP, LG, NEC, MDC |
| MQTT | `rumqttc` (already used) | fleet messages, Hisense control |
| Mobile bindings | `uniffi` | expose the companion core to Swift and Kotlin |
| Firmware | `embassy`, `esp-hal` | optional microcontroller sidecar |

## Proposed layers

The new work sits in four layers around the existing core. Each crate implements the existing `DeviceHandle` and capability traits, so the agent, the CLI and your Tauri platform pick it up with no new code paths.

&#91;embedded content: layer architecture · phone, host, protocols, hardware, core\]

The phone talks to the edge host, the host drives displays through protocol and hardware crates, and everything below the host speaks the core's traits.

### 1. Protocol layer: more displays

- One crate per wire protocol from the port table: `sicp`, `lgcmd`, `bravia`, `necpd`, `pjlink`, `androidtv`, `cast`, `jointspace`, `hisense`, `vizio`.
- A shared `tv-rs-serial` helper so every RS-232 protocol gets the same framing, timeouts, retries and RS-485 multi-drop addressing.
- One conformance run per crate against the existing mock and conformance suite.

### 2. Hardware layer: what is wired to the host

- `tv-rs-ddc`: DDC/CI for monitors (brightness, input, power, VCP read-back).
- `tv-rs-cec` v2: native libcec, CEC event subscriptions, multiple adapters.
- `tv-rs-ir`: IR blaster backends (Broadlink, plus GPIO IR on a Pi), with a learned-code store in TOML.
- `tv-rs-gpio`: relays (hard power cycle), presence sensors, ambient light for auto-brightness, temperature.
- `tv-rs-local` v2: display output control on Linux via DRM/KMS: modes, rotation, blanking.

### 3. Edge host layer: the box behind the screen

- `tv-rs-player`: a playback runtime over GStreamer or libmpv, with playlists as data, gapless switching and hardware decode. It plays, it does not schedule.
- `tv-rs-sync`: PTP or NTP clock discipline plus a start-at-timestamp protocol, so walls and multi-screen sites play in lockstep.
- `tv-rs-cache`: an offline content cache with content-hash verification, resumable downloads and size limits.
- `tv-rs-pop`: proof-of-play events and screenshots, written as JSONL like the existing audit log.
- `tv-rs-watchdog`: health checks, systemd notify and the hardware watchdog, reusing the agent's healing ladder.
- `tv-rs-hostd`: one daemon binary that wires the above to the agent, advertises itself over mDNS and serves a small local API for the phone.

### 4. Companion layer: the phone app's core

- `tv-rs-companion`: a pure-Rust core with no UI, exposed to Swift and Kotlin through `uniffi`.
- It handles discovery (mDNS plus SSDP), pairing flows for each vendor, remote keys, power and input, and talking to `tv-rs-hostd`.
- `tv-rs-provision`: first-boot setup over BLE using Improv, then QR-code pairing between phone and host.
- Platform constraints to design around: iOS needs the multicast networking entitlement, and Android needs a multicast lock for SSDP and mDNS.

No layer adds a database. Desired state, pins, tokens and caches stay as files, matching how tv-rs already stores pairing tokens and quirks.

## Roadmap

Build in four phases, ordered by signage value and by how cheaply each can be proven on real hardware.

&#91;embedded content: roadmap · 4 phases, 3 hardware gates\]

Each gate is a hardware test, not a date: the next phase starts only when the previous one has verified rows in `docs/hil-results.md`. Apple TV and other heavy consumer backends come after phase 4.

## Risks and design rules

The main risk is breadth without hardware proof: tv-rs already has zero hardware-verified rows, and 15 more crates would widen that gap.

- **Hardware first, then publish.** No new protocol crate ships to crates.io until it has at least one row in `docs/hil-results.md`. Buy or borrow one panel per protocol (a used Philips, LG commercial and NEC panel each cost little second-hand).
- **Port from specs, not code.** Treat Python and C# libraries as behaviour references. This avoids inheriting a reference library's licence and keeps the tv-rs style intact.
- **Keep native code optional.** libcec, GStreamer and libmpv pull C libraries. Each stays behind a feature flag, and the pure-Rust paths (SICP, PJLink, LG, NEC, Bravia) need no system libraries.
- **Apple TV is a cost trap.** The Companion and AirPlay 2 pairing crypto is large and moves with Apple's OS releases. Defer it until the other consumer backends are done.
- **Vendor changes break consumer protocols.** Android TV, Vizio and Hisense have changed pairing before. Route model and firmware defects through the existing quirks file, not code branches.
- **Mobile multicast limits.** iOS discovery needs Apple's multicast entitlement approval; plan a fallback where the edge host does discovery and the phone talks only to the host.
- **Bus factor.** The repo has one maintainer. A clear crate-per-protocol layout and the conformance suite are what make outside contributions safe; write `CONTRIBUTING` steps for adding a protocol before opening the work up.

## Sources

Found through web search on 2026-10-23; the pages were not opened individually, so treat details as leads to confirm.

- [tv-rs repository](https://github.com/Zakarialabib/tv-rs)
- [androidtvremote2](https://github.com/tronikos/androidtvremote2)
- [pyatv](https://pypi.org/project/pyatv/)
- [rust\_cast](https://docs.rs/crate/rust_cast)
- [philips-signage-display-sicp](https://github.com/aolde/philips-signage-display-sicp)
- [ha-philipsjs](https://pypi.org/project/ha-philipsjs/)
- [Sony Bravia Simple IP Control](https://pro-bravia.sony.net/remote-display-control/simple-ip-control/)
- [nec-pd-sdk](https://pypi.python.org/pypi/nec-pd-sdk)
- [Sharp/NEC external control manual](https://assets.sharpnecdisplays.us/documents/usermanuals/external-control-e988.pdf)
- [libLGTV\_serial](https://github.com/ehjortberg/libLGTV_serial)
- [lg-signage (npm)](https://www.npmjs.com/package/lg-signage)
- [pypjlink](https://pypi.org/project/pypjlink/), [aiopjlink](https://pypi.org/project/aiopjlink)
- [pyvizio](https://github.com/raman325/pyvizio)
- [hisensetv](https://hisensetv.readthedocs.io/)
- [python-broadlink](https://docsearch.algolia.com/mcp/docs/repo/mjg59/python-broadlink)
- [ddc-hi](https://docs.rs/ddc-hi)
- [statime](https://tweedegolf.nl/en/blog/72/announcing-statime-a-rust-ptp-implementation)
- [libmdns](https://lib.rs/crates/libmdns)
- [adb\_client](https://www.lib.rs/crates/adb_client)
- [cec-rs](https://lib.rs/crates/cec-rs)
- [Improv Wi-Fi](https://improv.hexdocs.pm/readme.html), [improv-wifi crate](https://docs.rs/improv-wifi)
