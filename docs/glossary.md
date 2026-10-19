# Glossary — tv-rs Architecture

Durable definitions for terms used across `docs/ADR-001-crate-publication.md`,
`docs/03-matrix.md` and `docs/01-playbook.md`. Engineering and naming rules are
not repeated here — see `00-rust-package-standard.md`.

## Workspace Terms

- **Facade (`tv-rs`)** — the root package (`src/lib.rs`). Re-exports the five
  protocol crates behind one interface and adds coordination: `discover()`,
  `identify_vendor()`, `DiscoveredTv`, `TvClient`, `TvVendor`, `TvError`. Holds no
  protocol logic of its own.
- **Protocol crate** — a focused crate in its own directory at the repository
  root (`discovery/`, `power/`, `tizen/`, `webos/`, `roku/`) implementing one
  Playbook section: `discovery` (§1), `power` (§2), `tizen` (§3), `webos` (§4),
  `roku` (§5).
- **Aggregate client (`TvClient`)** — enum over the vendor clients. Covers only
  operations that exist on all three vendors (`power_off`, `launch_app`); vendor
  specifics go through `as_tizen()` / `as_webos()` / `as_roku()`.
- **Vendor identification** — the documented heuristic in `identify_vendor()` that
  classifies an SSDP reply by substring match over its advertised fields. SSDP
  carries no authoritative vendor ID, so an unrecognised device is reported as
  `TvVendor::Unknown` rather than guessed at.
- **Virtual vs root manifest** — the workspace root is `[package] tv-rs` *and*
  `[workspace]` at once (the `tokio` pattern). A purely virtual manifest cannot be
  published, which is why the root is a real package.

## Protocol Terms (Playbook Mapping)

- **SSDP** (§1) — Simple Service Discovery Protocol. UDP multicast 1900,
  `M-SEARCH`, replies carry a `LOCATION` URL pointing at a UPnP device descriptor
  in XML. Unauthenticated.
- **UPnP device descriptor** — the XML document at the SSDP `LOCATION`: friendly
  name, manufacturer, model, serial number.
- **WoL / WoWLAN** (§2) — Wake-on-LAN magic packet: the target MAC repeated 16
  times behind 6 leading `0xFF` bytes (102 bytes total), sent to UDP 9.
- **DIAL** (§3) — Discovery And Launch. REST/XML on port 8080 used to start
  native Tizen apps (YouTube, Netflix) without a user prompt.
- **Tizen remote channel** (§3) — WebSocket on 8001 (legacy, plain) or 8002
  (TLS). Handshake is `ms.channel.connect`; without a saved token the TV shows a
  PIN/allow prompt and returns a token to reuse.
- **SSAP** (§4) — Second Screen Application Protocol. LG webOS JSON-RPC over
  WebSocket, 3000 plain or 3001 TLS, addressed by URIs such as
  `ssap://system.launcher/launch`. First connection triggers an on-screen prompt
  and returns a `client-key` to persist.
- **ECP** (§5) — External Control Protocol. Roku's documented, unauthenticated
  REST API on port 8060. Requests from the local subnet need no credentials.
- **mDNS / Bonjour** (§1) — multicast DNS on 5353. Used by AirPlay and Google
  Cast. **Not implemented**; represented by `DiscoveryMethod` only.
- **ARP scanning** (§1) — resolving IP → MAC before Wake-on-LAN. **Not
  implemented.**

## REA / Phase Terms

- **Phase 1** — the protocol implementations in this workspace. Complete:
  discovery, power, and the three vendor control channels, with tests.
- **Phase 2** — REA: JADX decompilation of the official Samsung SmartThings, LG
  ThinQ and Roku APKs to recover SSL-pinning bypasses, token/secret math, AES-GCM
  framing, hidden deep-link payloads and protobuf schemas. Not started.
- **Phase 3** — reintegration: implement the Phase 2 findings in place of the
  typed `Unsupported` errors currently returned by the affected code paths.
- **REA** — "Reverse Engineer Anything", a local MCP server and CLI for binary
  analysis. See `02-external-refs.md` §3.
- **Gap A** — SSL certificate pinning / WSS handshake handling for the
  self-signed certificates on Samsung 8002 and LG 3001.
- **Gap B** — enterprise token-generation math (TV MAC + hardcoded secret salt).
- **Gap C** — undocumented deep-link intents and payloads.
- **Gap D** — AES-GCM encrypted WebSocket framing on newer Samsung firmware.
- **Gap E** — proprietary Google Cast protobuf schemas.

All five gaps are described in `01-playbook.md` §10.

## License / Integration Terms

- **Direct dependency OK** — MIT / BSD / Apache-2.0 / MPL-2.0. Every dependency in
  this workspace qualifies: `tokio`, `serde`, `reqwest`, `thiserror`,
  `futures-util`, `async-trait`, `roxmltree`, `base64`, `url`, `native-tls`,
  `tokio-tungstenite`, `clap`, `wiremock`.
- **Sidecar / binary isolation** — only required for GPL/AGPL. Not needed here.
- **License file** — `LICENSE` (MIT) exists at the workspace root. Its absence was
  the trigger for `ADR-001`; that part is resolved.
- **Deployment** — distinct from crate publication and out of scope for it. The
  earlier `npm run deploy` reference described a profile-standard path that was
  never verified and has been dropped from `README.md`.
