# TV Control & Communications Playbook — Skeleton Mapping

Mapped against `pasted_content_2026-10-18_...` (Playbook §1-10).

## Phase 1 — Skeleton Crates (this workspace)

| Playbook § | Topic | Crate | Status |
|---|---|---|---|
| §1 | SSDP / mDNS / Bonjour / ARP | `discovery` | Skeleton |
| §2 | WoL / WoWLAN / power polling | `power` | Skeleton |
| §3 | Samsung Tizen (WS 8001/8002, DIAL 8080) | `tizen` | Skeleton |
| §4 | LG webOS SSAP (3000/3001) | `webos` | Skeleton |
| §5 | Roku ECP (8060 REST) | `roku` | Skeleton |

## Phase 2 — REA (excluded; separate child card)

§10 Gap A (SSL pinning / WSS handshake)  
§10 Gap B (token-generation math / MAC + salt)  
§10 Gap C (undocumented payloads / .pcap mapping)  
§10 Gap D (AES-GCM framing / firmware versions)  
§10 Gap E (Google Cast protobuf reverse)  

These require APK decompilation (SmartThings, ThinQ, Roku) via REA. Not stubbed as real implementations.

## Phase 3 — Deferred (not skeleton)

§6 Android TV ADB (5555)  
§7-8 DLNA / HLS / 206 Partial Content  
§9 Google Cast (TLS 8009 / protobuf) / AirPlay (RAOP / FairPlay)
