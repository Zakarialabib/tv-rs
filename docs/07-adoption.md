# App adoption contract

The standing contract between this workspace and any application that adopts it as
the source of truth for the TV domain. The first adopter is the Signage desktop
application; program status and workstream plans live in **that application's own
roadmap** (`docs/specs/ROADMAP-DESKTOP.md` §12–§13 there — deliberately not linked,
because it is another repository). This page holds only the durable contract: who
owns what, the no-duplication rules, the integration contracts, and the gates.

Registry note: crates.io serves **thirteen crates at `0.2.0`** today; `tv-rs-mdc` and
`tv-rs-app` are merged in this workspace and ride the `0.3.0` release (the registry
table in
[`03-matrix.md` §Published](03-matrix.md#published-cratesio-verified-2026-10-22-against-the-registry-api)
is the version truth).

## 1. Domain ownership

| Domain                            | Owner    | Examples                                                                                                                                                  |
| --------------------------------- | -------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **TV protocol and device domain** | `tv-rs`  | SSDP/mDNS discovery, vendor control, capabilities, pairing, TLS/pinning, sessions, retries/idempotency, reachability, local display probe, DLNA transport |
| **Signage domain**                | this app | display identity, display↔endpoint binding, schedules, content, campaigns, playlists, player, tenant/licensing, **media library and media server**        |
| **Transport glue**                | this app | Tauri commands, SQLite, event bus, React, OS keyring/file ACLs                                                                                            |
| **UI projection**                 | this app | `DisplayLinkStatus`, `TvCapabilities` DTO, evidence strings, remedy copy                                                                                  |

The goal is not "use `tv-rs` for a few commands":

> **`tv-rs` owns the TV domain. The app owns signage orchestration, persistence and UI.**

**Crate-first principle.** `tv-rs` is a pure-Rust TV-domain library, not a mirror
of any app's internals. Every adoption workstream must improve the crate's API,
tests and docs, and must replace the old application path rather than wrap it
forever. Signage is the first adopter, not the crate's design center: no Tauri,
`sqlx`, app DTOs or signage policy in `tv-rs`.

## 2. Fixed decisions

1. **No media library in `tv-rs`.** The app's media library, content catalog,
   thumbnails, HTTP server, ContentDirectory and 206 range serving stay
   app-owned. `tv-rs` gets **DLNA transport only** (renderer discovery,
   AVTransport SOAP, GENA); it never receives a `MediaLibrary` trait, a
   `MediaItem` type, a database handle or a content root.
2. **The agent is not adopted wholesale by default.** The app's scheduler and
   database remain the source of desired state; `tv-rs-agent` is a later
   decision (see the crate's own docs for the reconciler it offers).
3. **No Cast/AirPlay/Android TV control** until those crates exist; discovery
   sources only.
4. **Dependency policy is semver from the registry**: `tv-rs = "0.2"`; no git
   pin, no fork, no path override, no floating branch.

Non-goals that bound the whole program: no schedules, campaigns, content,
licensing or React UI in `tv-rs`; no Tauri, `sqlx`, `AppHandle`,
`DatabaseManager` or app DTOs in `tv-rs`.

**Success definition.** There is exactly one implementation of each TV protocol
in the dependency graph; the app keeps exactly one media library and one media
server and no `AcceptAnyTvCert`-style permissive TLS; vendor-specific branches
remain UI mapping, not protocol behaviour; `tv-rs` crates depend on no app,
Tauri or `sqlx`; `tv-rs` stays adoptable by a non-Tauri Rust host.

## 3. Standardization rules (the no-duplication contract)

These are review gates, not suggestions.

1. **One TV protocol, one implementation.** SSDP/mDNS TV discovery, Tizen WS,
   token storage, TLS pinning and DLNA **transport** exist only in `tv-rs`. The
   app keeps exactly one server-side media library/media server
   (ContentDirectory, 206, SSDP advertise); that is not a TV control protocol
   and does not move.
2. **One capability truth.** UI gating derives from `tv-rs-core::CapabilitySet`;
   `TvCapabilities` is a projection, not a second source. The commercial LFD
   surfaces (`PanelLock`, `Orientation`, `NoSignalStatus`, and behind the
   `supporting` feature `LampOrPanelHours`/`ScreenCapture`) follow the same
   projection rule: gate on the set, fetch with `as_panel_lock()` /
   `as_orientation()` / `as_panel_health()`, and treat `None` as "the protocol
   cannot" — MDC-backed handles say `Some`, the consumer channels say `None`,
   and neither invents data. The honest per-backend table:
   [03-matrix §Capability truth](03-matrix.md#capability-truth).
3. **One error model.** Use `tv_rs_core::TvError::remedy()` for routing. The
   facade's aggregate `tv_rs::TvError` is mapped once in the app adapter.
4. **One identity rule.** `TvClient::id()` is a host, not a stable device id.
   The app's `display_id` remains primary; `usn`/`serial_number`/MAC are
   evidence.
5. **One credential path.** Tizen tokens and webOS `client-key`s are stored
   through the vendor store or an app secret adapter; never in
   `DisplayLinkStatus`, logs or events.
6. **One session owner.** `TizenConnectionPool` and webOS session caching live
   in `tv-rs`; the app never opens raw TV sockets.
7. **Thin IPC.** Tauri commands validate input, call one adapter, map errors
   and return DTOs. No protocol code in `commands.rs`.
8. **No app types in `tv-rs`.** No `AppResult`, `DisplayLinkStatus`,
   `MediaItem`, `DatabaseManager`, `AppHandle` or `serde` shapes copied from
   the app.
9. **Feature-gated platform code.** `tv-rs-local` isolates Windows/Linux
   platform `unsafe`; `tv-rs-core` keeps `forbid(unsafe_code)`.
10. **Dependency policy.** Use the published `tv-rs = "0.2"` semver dependency;
    no git pin, fork or local path. No floating branch.
11. **Every extraction deletes the old path.** A move is not complete while
    both implementations are reachable.
12. **CI checks the boundary.** Add a grep/check that rejects the retired
    symbols and app-only dependencies in `tv-rs`.

The dependency direction is one sentence: the app depends on `tv-rs`; no
`tv-rs` crate may depend on an app, on Tauri, on `sqlx` or on React. The layer
diagram it fits into is [`01-architecture.md`](01-architecture.md); what each crate
implements today is [`03-matrix.md`](03-matrix.md).

## 4. Integration contracts

### 4.1 Identity

- `display_id` (app) is the primary key.
- `DiscoveredTv { vendor, ip, location, friendly_name, manufacturer, model_name,
serial_number, usn }` is evidence.
- `TvClient::id()` is a host string; never persist it as a device id.
- MAC/ARP is optional evidence for Wake; it is not a preference and not a
  display id.

### 4.2 Capabilities

- Source: `tv_rs_core::CapabilitySet`.
- App projection: `TvCapabilities` (`power`, `volume`, `mute`, `input`, `apps`,
  `keys`, `pairing_required`, `reason`).
- `pairing_required` and `reason` are app/auth state, not vendor capabilities.
- Runtime `Unsupported` wins over the static set.

### 4.3 Errors and remedies

- `DeviceHandle` methods return `tv_rs_core::TvError` with `remedy()`.
- Facade inherent methods return aggregate `tv_rs::TvError`; map once to the
  app's state model and action copy.
- Do not create a second TV error enum in an app plugin.

### 4.4 Credentials

- Tizen: `SamsungTokenStore`, keyed by the app's display id; migrate legacy
  IP-keyed entries.
- webOS: `save_client_key` / `load_client_key`, keyed by display id.
- Roku: no pairing, no TLS.
- TLS: TOFU first (`TlsPolicy::TrustOnFirstUse`), then
  `TlsPolicy::pinned(fingerprint)`; `AcceptInvalid` is banned from shipped code.
- Storage: OS keyring preferred; otherwise an app-config file with restrictive
  ACL. Never in link DTOs, events or logs.

### 4.5 Media

- **The app owns the media library.** `MediaItem`, database handles,
  thumbnails, content auth, ContentDirectory, 206 ranges and the media-server
  advertisement stay in the app.
- **`tv-rs-dlna` owns DLNA transport only:** renderer discovery, AVTransport
  SOAP and GENA. It receives a URI/metadata; it never receives a library,
  content root or database handle.
- There is **no `MediaLibrary` trait in `tv-rs`**, and this program does not
  adopt `tv-rs-media`.
- The app keeps playlist/caching/retry policy; the transport crate must not
  encode signage semantics.

## 5. Security requirements

- No permissive TLS in shipped code. Permissive verifiers are deleted on
  adoption, not ported upstream.
- Pin by fingerprint, not by "accept anything"; rotate with a visible
  diagnostic event.
- Tokens/client keys are bearer credentials: redacted in logs, never serialized
  into UI DTOs.
- SSDP/M-SEARCH responses are parsed with bounded sizes; no unbounded XML
  entity expansion.
- Media paths are confined to the library root; reject traversal (`..`) and
  absolute paths.
- `tv-rs-local` platform `unsafe` is isolated, documented and tested; no
  `unsafe` in `tv-rs-core`.
- Dependency review covers MIT/Apache compatibility, MSRV alignment, and
  duplicate WebSocket/TLS stacks.

## 6. Verification gates

| Gate           | Evidence                                                                  |
| -------------- | ------------------------------------------------------------------------- |
| Contract       | the app's command-contract checker                                        |
| Types          | the app's TypeScript typecheck                                            |
| TV adapter     | `cargo test -p tv-rs-tizen` / `tv-rs-webos` / `tv-rs-roku`                |
| Local probe    | `cargo test -p tv-rs-local`                                               |
| DLNA transport | `cargo test -p tv-rs-dlna`; cast/stop/GENA parity                         |
| Dependency     | `cargo tree -i native-tls` empty; `cargo tree -d` reviewed                |
| Boundary       | grep checks for retired symbols and app-only dependencies                 |
| Hardware       | one device per vendor in the app's matrix; "unverified" stated explicitly |

## 7. What this page is not

Not a program plan and not a changelog. Adoption workstream sequencing, per-
workstream acceptance checklists and extracted-code history live in the adopting
application's repository; landed `tv-rs` changes are recorded in
[`CHANGELOG.md`](../CHANGELOG.md), and open `tv-rs` work belongs in
[`06-plan.md`](06-plan.md) — if an adoption needs a crate change, it goes
through the plan.
