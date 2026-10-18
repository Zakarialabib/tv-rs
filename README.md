# tv-rs — TV Control & Communications Workspace

Rust workspace for TV control protocols. Phase 1 skeleton (Phase 2 = REA, Phase 3 = reintegration).

## Scope (IN)
- `discovery`: SSDP / mDNS / Bonjour / ARP scanning (Playbook §1, §7)
- `power`: WoL / WoWLAN / power state polling (Playbook §2)
- `tizen`: Samsung Tizen — WebSocket (8001/8002) + DIAL (§3)
- `webos`: LG webOS — SSAP / JSON-RPC (§4)
- `roku`: Roku OS — ECP REST (§5)

## Explicitly OUT (Phase 2 REA — separate card)
- SSL pinning bypass, token-generation math (§10 Gap B)
- AES-GCM encrypted framing (§10 Gap D)
- Hidden deep-link payloads (§10 Gap C)
- Protobuf schema reverse (§10 Gap E)
- ADB over TCP/IP (§6), DLNA/HLS (§7-8), Cast/AirPlay (§9)

Auth/crypto insertion points are `unimplemented!()` trait stubs only.

## Build
```
cargo check --workspace
npm run deploy   # profile-standard deploy path
```

Workspace: Windows/MSYS (`C:\laragon\www\tv-rs`). See profile notes for deploy rules.
