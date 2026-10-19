# External References — SDKs, Crates, Tools & Research

Everything outside this repository that informed the design: official protocol
docs, prior-art crates, the REA reverse-engineering tooling, and the research
findings with their outcomes. The engineering rules derived from these sources
live in `00-rust-package-standard.md`; current status lives in `03-matrix.md`.

---

## 1. Official SDK / Protocol Documentation

| Reference | URL | What it provides | Relevant crate |
|---|---|---|---|
| Roku External Control Protocol (ECP) | https://developer.roku.com/dev/docs/external-control-api | Official REST endpoints (`/query/device-info`, `/keypress`, `/launch`), SSDP `M-SEARCH` on UDP 1900, XML response schemas | `roku`, `discovery` |
| Samsung SmartThings SDK (Core) | https://developer.smartthings.com/docs/sdks/core | Wrapper for the SmartThings cloud API; reference for token/auth handling patterns | `tizen` |
| Samsung Developer / SmartThings | https://developer.samsung.com/smartthings | Platform ecosystem and device-connection patterns | `tizen`, `discovery` |
| Samsung Smart TV SDK (CodeLab) | https://developer.samsung.com/codelab/smartthings/device-sdk.html | Device SDK integration examples | `tizen` |
| LG Smart TV SDK release notes | https://community.brightcove.com/product-updates/introducing-smart-tv-sdks-for-roku-samsung-and-lg-devices-246 | Web-based SDK announcement; SSAP / webOS pointers | `webos` |

Note on coverage: the Tizen remote WebSocket channel (8001/8002) and the webOS
SSAP service are **not** officially documented for third-party use. Samsung and
LG publish SmartThings/ThinQ cloud APIs instead. The local `ms.channel.connect`
and `ssap://` protocols are community-reverse-engineered, which is why the §10
gaps in `01-playbook.md` exist.

---

## 2. Prior-Art Rust Crates

| Crate | Source | Notable dependencies | How we relate to it |
|---|---|---|---|
| `samsung-tv` v0.1.1 | https://docs.rs/samsung-tv | `tokio-tungstenite` ^0.24, `native-tls` ^0.2, `reqwest` ^0.12, `serde`, `base64`, `thiserror`, `tracing`, `url`, `dirs` | **Adopted.** Closest architecture match for `tizen`: WSS via `tokio-tungstenite`, TLS via `native-tls`, REST via `reqwest`, plus SSDP discovery. We took the dependency set (minus `tracing` and `dirs`, which we do not need) rather than re-deriving TLS/WebSocket handling. |
| `roku` | Community ECP docs only | — | No mature crate found. ECP is fully documented and unauthenticated, so `roku` was written directly against the public spec using `reqwest` + `roxmltree`. |

---

## 3. REA — Reverse Engineering Tooling

REA ("Reverse Engineer Anything", `morluto/rea`, npm `rea-agents`, MIT) is a
local MCP server + CLI for inspecting binaries, applications and runtime
behaviour. It has ~209 documented MCP tools spanning native binaries (Hopper /
Ghidra / IDA), JavaScript/Electron, .NET, Android, firmware, WASM and EVM, plus
runtime capture (browser scenarios, ADB, pcap, signal recordings).

Analysis runs locally and every result carries its evidence and limitations.

### Tools relevant to Playbook §10

| Tool | Purpose |
|---|---|
| `mcp__rea__inspect_android_package` | APK package, SDK, permissions, manifest |
| `mcp__rea__search_android_classes` | Search decompiler class-name rows by substring |
| `mcp__rea__inspect_android_method` | Decompile one method (smali fallback) |
| `mcp__rea__batch_decompile` | Decompile multiple symbols concurrently |
| `mcp__rea__decompile_jeb_item` | Decompile a type or method to pseudo-code |
| `mcp__rea__procedure_pseudo_code` | Pseudo-code by symbol/address (Ghidra fallback) |
| `mcp__rea__decode_android_resources` | `apktool d --no-src`; extract string resources |
| `mcp__rea__pull_adb_package` | Pull an installed APK from a connected device |
| `mcp__rea__list_adb_packages` | Inventory installed packages |

**Limitation:** REA is analysis-only with respect to a target APK — it does not
execute the app. Decompilation yields pseudo-code, not byte-accurate behaviour, so
REA output must be validated against a real device before it is trusted.

---

## 4. Research Findings and Outcomes

Sources consulted on 2026-10-18 while setting direction, recorded with what we
actually did about each, so the recommendations are not re-litigated.

| Finding | Source | Outcome |
|---|---|---|
| Rust binaries are substantially harder to reverse-engineer than C because of compiler optimisations and inlining. | arXiv 2507.18792, *Decompiling Rust* | Not actionable directly, and the original framing ("our stubs leak nothing") is now moot: the crates contain real implementations and there are no `unimplemented!()` stubs left. Retained only as context. |
| A proven Rust Tizen control architecture exists in the `samsung-tv` crate (see §2). | docs.rs | **Adopted** for `tizen`, including the TLS and WebSocket stack choice. |
| Use `socket2` for UDP multicast in `discovery`. | Derived from the SmartThings/mDNS material | **Rejected.** `tokio::net::UdpSocket` already provides `join_multicast_v4` and integrates with the async runtime this workspace is built on. Adding `socket2` would have introduced a second socket abstraction for no capability gain. `discovery` uses `tokio` + `roxmltree` instead. |
| Add `reqwest` to `roku` for REST and XML. | Roku ECP docs | **Adopted** — `roku` uses `reqwest` plus `roxmltree` for the XML responses. |
| `power` needs no external crate; UDP is sufficient. | Playbook §2 | **Adopted** — `power` uses `tokio` only. |
| JADX-based decompilation of the SmartThings / ThinQ / Roku APKs is the prerequisite for the §10 authentication gaps. | REA tooling | **Pending** — this is Phase 2, tracked in `03-matrix.md`. Until it lands, the affected code paths return typed `Unsupported` errors. |
