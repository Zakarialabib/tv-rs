# Playbook → Crate Mapping

Compact view of how the Playbook sections in `01-playbook.md` map onto crates and
phases. For the per-crate detail, test counts and next actions, see `03-matrix.md`.

## Phase 1 — Protocol crates (this workspace)

| Playbook § | Topic | Crate | Status |
|---|---|---|---|
| §1 | SSDP / mDNS / Bonjour / ARP | `tv-rs-discovery` | SSDP and UPnP descriptor parsing implemented; mDNS and ARP pending |
| §2 | WoL / WoWLAN / power polling | `tv-rs-power` | Implemented |
| §3 | Samsung Tizen (WS 8001/8002, DIAL 8080) | `tv-rs-tizen` | Implemented except §10 Gaps A, B and D |
| §4 | LG webOS SSAP (3000/3001) | `tv-rs-webos` | Implemented except §10 Gap A |
| §5 | Roku ECP (8060 REST) | `tv-rs-roku` | Implemented |
| — | Facade | `tv-rs` (root `src/`) | Implemented: discovery orchestration, vendor identification, aggregate client |

## Phase 2 — REA (not started)

| Gap | Topic |
|---|---|
| §10 A | SSL certificate pinning / WSS handshake handling |
| §10 B | Token-generation math (MAC + secret salt) |
| §10 C | Undocumented deep-link payloads (`.pcap` / `.har` mapping) |
| §10 D | AES-GCM encrypted framing across firmware versions |
| §10 E | Google Cast protobuf schema reverse |

These require APK decompilation of the official SmartThings, ThinQ and Roku apps
via REA. Until it lands, the affected code paths return typed `Unsupported`
errors — they are not stubbed with panics. See `02-external-refs.md` §3.

## Phase 3 — Deferred (no crate yet)

| Playbook § | Topic | Planned crate |
|---|---|---|
| §6 | Android TV / Google TV over ADB (5555) | `tv-rs-adb` |
| §7–8 | DLNA media rendering, HLS and HTTP 206 seeking | `tv-rs-media` |
| §9 | Google Cast (TLS 8009 / protobuf), AirPlay (RAOP / FairPlay) | `tv-rs-cast` |
