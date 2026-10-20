# TV-RS Adoption & Domain Consolidation Spec

**Status:** **U0 COMPLETE · U1 DESIGN DECIDED 2026-10-20 · U2 EXTRACTED, SOFTWARE-VERIFIED** —
dependency resolved, inventory recorded, roadmap linked. U1's architecture is settled
(below); U2's crate extraction has landed and passed software gates. Windows hardware
and Linux parity remain before U2 is fully closed.

### U1 architecture decisions (the interesting part)

The app's `plugin-samsung-remote` is 1,582 LOC across 8 files. After U1 it is a **thin
IPC adapter**: `commands.rs` (7 commands) + `models.rs` (DTOs) + `error.rs` +
`lifecycle.rs` + `lib.rs`, plus one new `adapter.rs` that is the _only_ place tv-rs
types are mapped to the app's IPC surface. Three protocol files are deleted outright:

| Deleted        | LOC | Replaced by                                                     |
| -------------- | --- | --------------------------------------------------------------- |
| `remote.rs`    | 570 | `tv_rs_tizen::{TizenClient, TizenSession, TizenConnectionPool}` |
| `tokens.rs`    | 214 | `tv_rs_tizen::SamsungTokenStore` (drop-in)                      |
| `discovery.rs` | 274 | `tv_rs_tizen::discover_samsung`                                 |

**Decision 1 — TLS: TOFU replaces accept-any. The security debt is real and it is
the point of U1.** The app ships a live `rustls::client::danger::ServerCertVerifier`
(`AcceptAnyTvCert`, `remote.rs:181`) plus `danger_client_config()`. Both are deleted.
The secure path uses `TlsPolicy::TrustOnFirstUse(cert_sink())`: the first certificate
is recorded, any later mismatch is rejected — TOFU with pinning, exactly the spec's
"TOFU first, then pinned" requirement.

Note: `tv-rs-transport` _also_ contains an `AcceptAnyTvCert`, but it is **private**
(`struct`, not `pub`) and reachable only through `TlsPolicy::AcceptInvalid` — a gated
escape hatch, not a default. U1 simply never constructs it. The app-side verifier is
the one that had to go; the upstream one stays unused and unreachable.

**Decision 2 — arbitrary keys stay, with no upstream change.** The app's UI accepts
any `KEY_*` string (`validate_key` pattern-checks rather than whitelisting). tv-rs
exposes a closed 21-variant `TizenRemoteKey` enum. Rather than restrict the UI or fork
upstream, the adapter sends the app's existing `key_payload()` frame through
`TizenSession::send_raw()`. `send_raw` exists precisely for this, so open decision #4
resolves to **"no upstream change"** and the UI keeps its flexibility.

**Decision 3 — the app's hand-rolled plain→secure fallback is already upstream.**
`remote.rs::open_channel` tries `ws://8001` then `wss://8002`. tv-rs's
`open_channel` iterates `client.port_candidates()` with the same ordering and, via
`should_try_next_port`, only falls through on _transport_ refusal — a certificate or
protocol verdict is final. So the fallback is not re-implemented in the adapter; it
is inherited, and the adapter gets the stricter "auth answer is final" semantics for
free.

**Decision 4 — error mapping happens once, in `adapter.rs`.** `commands.rs` stays
free of protocol knowledge: each command validates input, calls one tv-rs function,
and maps the result. `TizenError` → `SamsungError` lives in one function.

**Behaviour parity verified before deleting anything:** the app's `TokenStore::upsert`
refuses to clobber a stored token with an empty one. `tv_rs_tizen::SamsungTokenStore::
upsert` (token_store.rs:99-103) carries the identical guard
(`if !token.is_empty() || entry.token.is_empty()`), and both stores quarantine a
corrupt file to `.corrupt`. The swap is behaviour-preserving, not just API-compatible.

**Open risk — the pairing wait window.** The app waits ≤30 s for the human to accept
the on-TV prompt (`PAIR_WAIT`). `TizenSession::connect` uses tv-rs's own
`HANDSHAKE_TIMEOUT`; the adapter must confirm pairing still allows a human-sized
window, or the pairing flow regresses.

> **U0 premise correction (2026-10-20).** This spec assumed `tv-rs` was unpublished
> and needed a git-SHA pin. It is **published on crates.io at 0.2.0** — the facade and
> all eight protocol crates are in the registry cache — so the app depends on the
> semver `tv-rs = "0.2"`. No git pin, no fork, no local path. The facade already
> exposes the surface U1–U4 need: `discover`, `discover_with`, `identify_vendor`,
> `TvClient`, `TvError`/`Result`, `DiscoveredTv`, `TvVendor`, plus re-exports of
> `core`/`discovery`/`power`/`roku`/`tizen`/`webos`.
> **Scope:** desktop TV/display lane; `tv-rs` as the source of truth for the TV domain.
> **Roadmap companion:** §12 (verified `tv-rs` state) and §13 (what is coming) of the
> application's own `docs/specs/ROADMAP-DESKTOP.md`. It is not linked: that file lives
> in another repository, and a relative path into it renders as a dead link here.
> **Reference implementation:** `tv-rs` GitHub `fedbf07`, MIT, MSRV `1.89`; app MSRV `1.94`.

---

## 1. Problem

TV knowledge is currently split across five places:

| Place                                        | What it owns today                                                     | Problem                                                                        |
| -------------------------------------------- | ---------------------------------------------------------------------- | ------------------------------------------------------------------------------ |
| `plugin-samsung-remote`                      | Tizen SSDP, token store, WS pool, key/app payloads, permissive TLS     | Duplicates `tv-rs-tizen`; `danger_client_config` is a security debt            |
| `plugin-dlna`                                | DLNA control point, GENA, SSDP advertisement                           | Transport implementation lives in the app; `tv-rs` has no DLNA transport crate |
| `src-tauri/src/network/dlna_media_server.rs` | Signage media library, MediaServer, ContentDirectory, 206 ranges, SSDP | **Not a `tv-rs` target.** Content and media library stay in Signage            |
| `plugin-display/src/hardware/probe.rs`       | Local EDID/WMI/DRM display truth                                       | `tv-rs` has no local-display layer                                             |
| `network/{health,lan_display_discovery}.rs`  | Reachability, SSDP + mDNS union                                        | `tv-rs` has TCP power probe and SSDP only                                      |

This creates three failures:

1. **Drift** — two SSDP paths, two token stores, two TLS policies and two DLNA implementations.
2. **Inconsistent domain rules** — capability truth, pairing, retry/idempotency and health live in different layers.
3. **Security exposure** — the app still carries `AcceptAnyTvCert` / `danger_client_config`.

The goal is not "use `tv-rs` for a few commands". The goal is:

> **`tv-rs` owns the TV domain. The app owns signage orchestration, persistence and UI.**

> **Crate-first principle.** `tv-rs` should be the best pure-Rust TV-domain library
> for our scenarios, not a mirror of app internals. Every workstream must improve
> the crate's API, tests and docs as it moves code, and must replace or enhance
> the old app path rather than wrap it forever. Signage is the first adopter, not
> the crate's design center: no Tauri, `sqlx`, app DTOs or signage policy in
> `tv-rs`.

---

## 2. Decision

### 2.1 Domain ownership

| Domain                            | Owner    | Examples                                                                                                                                                  |
| --------------------------------- | -------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **TV protocol and device domain** | `tv-rs`  | SSDP/mDNS discovery, vendor control, capabilities, pairing, TLS/pinning, sessions, retries/idempotency, reachability, local display probe, DLNA transport |
| **Signage domain**                | this app | display identity, display↔endpoint binding, schedules, content, campaigns, playlists, player, tenant/licensing, **media library and media server**        |
| **Transport glue**                | this app | Tauri commands, SQLite, event bus, React, OS keyring/file ACLs                                                                                            |
| **UI projection**                 | this app | `DisplayLinkStatus`, `TvCapabilities` DTO, evidence strings, remedy copy                                                                                  |

### 2.2 Fixed decision: no media library in `tv-rs`

The Signage media library, content catalog, thumbnails, HTTP server, ContentDirectory and 206 range serving remain in this app. `tv-rs` gets **DLNA transport only** (renderer discovery, AVTransport SOAP, GENA); it never receives a `MediaLibrary` trait, a `MediaItem` type, a database handle or a content root.

### 2.3 Non-goals

- Moving schedules, campaigns, content, media library, licensing or React UI into `tv-rs`.
- Putting Tauri, `sqlx`, `AppHandle`, `DatabaseManager`, `MediaItem` or app DTOs into `tv-rs`.
- Adopting `tv-rs-agent` wholesale in this program. The app scheduler/DB remain the source of desired state.
- Implementing Cast/AirPlay/Android TV control in this program. Discovery-only remains the D7 scope.
- Publishing every crate in one release. Publishing follows the extraction order.

### 2.4 Success definition

1. There is exactly one implementation of each TV protocol in the dependency graph.
2. The app contains no `AcceptAnyTvCert`, no second Tizen token store and no second SSDP scanner; the app keeps exactly one media library and one media server.
3. The app's vendor-specific branches are limited to display/UI mapping, not protocol behavior.
4. `tv-rs` crates have no dependency on this app, Tauri or `sqlx`.
5. `tv-rs` is adoptable by a non-Tauri Rust host: public API documented, examples/tests run without app code, and no signage-only semantics in the crate.
6. The app still passes its contract, type and cargo gates after each extraction.

---

## 3. Target architecture

```text
                         +-----------------------------+
                         |        tv-rs facade         |
                         |  discovery, TvClient,       |
                         |  capabilities, errors       |
                         +--------------+--------------+
                                        |
        +----------------+--------------+--------------+----------------+
        |                |                             |                |
  tv-rs-core       tv-rs-discovery                vendor crates     tv-rs-power
  capabilities     SSDP + mDNS                  tizen/webos/roku    WOL + probes
  DeviceHandle     descriptors                  pairing/sessions
  TvError          local display probe*         TLS via transport
        |                |                             |                |
        +----------------+--------------+--------------+----------------+
                                        |
                         +--------------+--------------+
                         |        tv-rs-dlna*      |
                         | renderer discovery,     |
                         | AVTransport, GENA       |
                         | no content store        |
                         +-------------------------+

  * new crate in this program

App-owned (never in tv-rs):
  media library, MediaItem, ContentDirectory, 206 media server, SSDP advertise,
  database, thumbnails, playlists, schedules, player, UI

App adapters:
  plugin-display  -> link/binding projection, UI state, IPC
  plugin-dlna     -> thin adapter over tv-rs-dlna transport
  dlna_media_server -> app-owned library + HTTP/UPnP media server
  network/health  -> history/DB; probe delegated to tv-rs-power
  network/lan_display_discovery -> app union; sources delegated to tv-rs
```

### 3.1 Dependency direction

```text
app crates  --->  tv-rs facade / protocol crates
tv-rs crates -X-> app crates
tv-rs crates -X-> tauri / sqlx / React
```

No `tv-rs` crate may depend on an app crate. The app may depend on `tv-rs`; never the reverse.

### 3.2 Crate map

| Crate                            | Current state     | Program role                                                                       |
| -------------------------------- | ----------------- | ---------------------------------------------------------------------------------- |
| `tv-rs-core`                     | published `0.2.0` | capability traits, `DeviceHandle`, `TvError`/`Remedy`, `CommandId`, observed state |
| `tv-rs-discovery`                | published `0.2.0` | SSDP + descriptor parsing; add generic mDNS source later                           |
| `tv-rs-power`                    | published `0.2.0` | Wake-on-LAN + TCP power/health probe                                               |
| `tv-rs-transport`                | published `0.2.0` | TLS policy, pinning, frame reader                                                  |
| `tv-rs-tizen` / `webos` / `roku` | published `0.2.0` | vendor backends, pairing, sessions                                                 |
| `tv-rs` facade                   | published `0.2.0` | stable `discover` / `TvClient` / `DeviceHandle` surface; semver dependency         |
| `tv-rs-local`                    | **new**           | local EDID/connector probe, adapted from `plugin-display/hardware/probe.rs`        |
| `tv-rs-media`                    | not adopted       | **No media library in tv-rs.** Signage keeps the media server and content          |
| `tv-rs-dlna`                     | **new**           | DLNA transport: renderer discovery, AVTransport, GENA; no content store            |
| `tv-rs-agent`                    | not published     | later; app supplies desired state, does not adopt `Reconciler` now                 |
| `tv-rs-cec`                      | not published     | later; keep hidden until adapter probe exists                                      |

---

## 4. Standardization rules (no-duplication contract)

These are review gates, not suggestions.

1. **One TV protocol, one implementation.** SSDP/mDNS TV discovery, Tizen WS, token storage, TLS pinning and DLNA **transport** exist only in `tv-rs`. The app keeps exactly one server-side media library/media server (ContentDirectory, 206, SSDP advertise); that is not a TV control protocol and does not move.
2. **One capability truth.** UI gating derives from `tv-rs-core::CapabilitySet`; `TvCapabilities` is a projection, not a second source.
3. **One error model.** Use `tv_rs_core::TvError::remedy()` for routing. The facade's aggregate `tv_rs::TvError` is mapped once in the app adapter.
4. **One identity rule.** `TvClient::id()` is a host, not a stable device id. The app's `display_id` remains primary; `usn`/`serial_number`/MAC are evidence.
5. **One credential path.** Tizen tokens and webOS `client-key`s are stored through the vendor store or an app secret adapter; never in `DisplayLinkStatus`, logs or events.
6. **One session owner.** `TizenConnectionPool` and webOS session caching live in `tv-rs`; the app never opens raw TV sockets.
7. **Thin IPC.** Tauri commands validate input, call one adapter, map errors and return DTOs. No protocol code in `commands.rs`.
8. **No app types in `tv-rs`.** No `AppResult`, `DisplayLinkStatus`, `MediaItem`, `DatabaseManager`, `AppHandle` or `serde` shapes copied from the app.
9. **Feature-gated platform code.** `tv-rs-local` isolates Windows/Linux platform `unsafe`; `tv-rs-core` keeps `forbid(unsafe_code)`.
10. **Dependency policy.** Use the published `tv-rs = "0.2"` semver dependency; no git pin, fork or local path. `tv-rs-local` and `tv-rs-dlna` are published at `0.2.0`. No floating branch.
11. **Every extraction deletes the old path.** A move is not complete while both implementations are reachable.
12. **CI checks the boundary.** Add a grep/check that rejects the retired symbols and app-only dependencies in `tv-rs`.

---

## 5. Workstreams

### U0 - Baseline, dependency and inventory

**Current:** `tv-rs = "0.2"` is already a dependency of `plugin-samsung-remote`; the spec and `ROADMAP-DESKTOP.md` §12/§13 are the current plan.

**Target:**

- Keep the published `tv-rs = "0.2"` semver dependency in the lane-owned manifest; no git pin or path override.
- Record `cargo tree`, license, MSRV and duplicate `tokio-tungstenite` lines.
- Keep this spec linked from `ROADMAP-DESKTOP.md` §12/§13.
- Inventory every protocol symbol in the app before deleting anything.

**Acceptance (all green 2026-10-20):**

- [x] `cargo tree -p plugin-samsung-remote -i tv-rs` resolves `tv-rs v0.2.0` + the seven
      protocol crates (`core`, `discovery`, `power`, `roku`, `tizen`, `transport`, `webos`).
- [x] `cargo tree -i native-tls` → **empty**. The repo's rustls-only policy holds; tv-rs
      brings no native-tls.
- [x] `cargo tree -d` records **two** `tokio-tungstenite` versions: `0.24.0` (the app's
      MCP client) and `0.30.0` (`tv-rs-transport`). Both are rustls-only — no TLS-stack
      duplication, just two WS majors to watch.
- [x] No app code depends on a local `C:\laragon\www\tv-rs` path (grep for `path =` + `tv-rs` across every manifest returns nothing).
- [x] The spec is linked from `ROADMAP-DESKTOP.md` §12.

**Baseline recorded:**

| Fact         | Value                                                                                                                                                  |
| ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Dependency   | `tv-rs = "0.2"` (crates.io) in `plugin-samsung-remote/Cargo.toml`                                                                                    |
| Upstream     | `https://github.com/Zakarialabib/tv-rs.git`; spec SHA `fedbf07d0fafd9a927a155e98cfc44f4572329b2` (note: `origin/main` is 2 commits ahead at `030600d`) |
| License      | MIT (workspace) — compatible with the app's `MIT OR Apache-2.0`                                                                                        |
| MSRV         | `1.89` (app is `1.94` — no conflict)                                                                                                                   |
| TLS          | rustls only; `native-tls` absent                                                                                                                       |
| Duplicate WS | `tokio-tungstenite` 0.24.0 (app) + 0.30.0 (tv-rs-transport)                                                                                            |

**Protocol-symbol inventory (delete targets for U1–U5):**

| Symbol                                    | Location                                                                          | Workstream |
| ----------------------------------------- | --------------------------------------------------------------------------------- | ---------- |
| `AcceptAnyTvCert`, `danger_client_config` | `plugin-samsung-remote/src/remote.rs:181,233,241,262,563`                         | U1         |
| `parse_ssdp_reply`                        | `plugin-samsung-remote/src/discovery.rs:89` (callers :57,194,204,212)             | U1         |
| `TokenStore`, `ConnectionManager`         | `plugin-samsung-remote/src/{tokens,remote}.rs`                                    | U1         |
| `AVTransport` / `GENA` control point      | `plugin-dlna/src/{discovery,soap,gena,gena_client,gena_parse,metadata,models}.rs` | U3         |
| Local EDID/WMI/DRM probe                  | `plugin-display/src/hardware/probe.rs` (7 tests)                                  | U2         |
| TCP/HTTP reachability probe               | `src-tauri/src/network/health.rs`                                                 | U4         |

**App-owned, never moving:** `dlna_media_server.rs`, `plugin-dlna/src/advertise.rs`,
`plugin-dlna/src/manager.rs`, all `MediaItem`/content code,
`plugin-display/src/hardware/link.rs`, `network/mdns.rs`, `network/discovery.rs`, and
every UI/scheduling/tenancy surface.

**Non-goal:** changing UI behavior.

---

### U1 - Replace the Samsung plugin with `tv-rs-tizen`

**Current source:** `src-tauri/crates/plugin-samsung-remote/src/{remote,tokens,discovery,commands}.rs`.

**Target:** `tv-rs-tizen` through the `tv-rs` facade.

**Mapping:**

| Current                                    | Replace with                                                                      |
| ------------------------------------------ | --------------------------------------------------------------------------------- |
| `AcceptAnyTvCert` / `danger_client_config` | `TlsPolicy::TrustOnFirstUse` then `TlsPolicy::pinned`; delete the permissive path |
| `TokenStore`                               | `SamsungTokenStore` keyed by app `display_id`                                     |
| `ConnectionManager`                        | `TizenConnectionPool`                                                             |
| `discover` / `parse_ssdp_reply`            | `tv_rs::discover()` / `tv-rs-discovery`                                           |
| `validate_key` / `key_payload`             | `TizenRemoteKey` for the common set; add a raw-key delta only if the UI needs it  |
| `resolve_app_id`                           | `tv_rs_tizen::resolve_app_id`                                                     |
| `ChannelEvent`                             | `parse_channel_connect_response` / `is_unauthorized` plus one app error mapping   |
| `samsung_*` commands                       | Keep the seven command names as thin adapters                                     |

**Delta to upstream (only if required):**

- `TizenClient::send_key_raw(&str)` or `Key::Raw` for `KEY_*` codes outside the enum. The current UI accepts arbitrary `KEY_` values; `tv-rs` exposes a closed enum.
- Optional `TizenError` classification for `ms.error` frames if the facade does not already expose it.

**Acceptance:**

- [ ] `samsung_discover_devices`, `samsung_pair`, `samsung_send_key`, `samsung_launch_app`, `samsung_disconnect`, `samsung_list_paired`, `samsung_forget_paired` keep their IPC names and payload shapes.
- [ ] Pairing survives an app restart; the token is loaded by `display_id`, not IP.
- [ ] A TV with a self-signed certificate pairs via TOFU and is pinned afterwards.
- [ ] `danger_client_config` and `AcceptAnyTvCert` are absent from the tree.
- [ ] Only one Tizen WS pool exists in the dependency graph.
- [ ] No UI changes are required for this workstream.

**Risk:** `SamsungTokenStore` persists by string key; verify IP-to-display-id migration and legacy entries before deleting the old store.

---

### U2 - Move the local display probe to `tv-rs-local`

**Current source:** `src-tauri/crates/plugin-display/src/hardware/probe.rs` (Windows WMI + `EnumDisplayDevicesW`, Linux DRM fallback, EDID parsing, `tv_confidence`, 7 tests).

**Target:** new `tv-rs-local` crate.

**API (implemented 2026-10-20):**

```rust
pub enum Attachment { Internal, Hdmi, DisplayPort, Dvi, Vga, Wireless, Unknown }

pub struct ProbeInfo {
    pub system_name: Option<String>, // connector key; not the app display_id
    pub width: u32,
    pub height: u32,
    pub vendor: Option<String>,
    pub model: Option<String>,
    pub serial: Option<String>,
    pub attachment: Attachment,
    pub refresh_rate: Option<u32>,
    pub bits_per_pixel: Option<u32>,
    pub physical_width_cm: Option<f64>,
    pub physical_height_cm: Option<f64>,
    pub brightness: Option<u32>,
    pub tv_confidence: f32,
    pub is_tv: bool,
    pub evidence: Vec<String>,
}

pub trait DisplayProbe {
    fn probe(&self) -> Vec<ProbeInfo>;
}

pub struct SystemProbe;
pub fn probe_displays() -> Vec<ProbeInfo>;
pub async fn probe_displays_async() -> Vec<ProbeInfo>;
pub fn probe_for<'a>(probes: &'a [ProbeInfo], system_name: Option<&str>, index: usize)
    -> Option<&'a ProbeInfo>;
pub fn fallback(system_name: Option<&str>, width: u32, height: u32, refresh: Option<u32>)
    -> ProbeInfo;
```

The first cut deliberately keeps the field names and units the app already maps
(`system_name`, `refresh_rate`, `physical_*_cm`, `bits_per_pixel`, `brightness`).
Renaming to `id`/`refresh_hz`/`physical_inches` would be app churn without a
consumer benefit; the crate docs state the units. `ProbeInfo` is
`Serialize`/`Deserialize`/`PartialEq` so downstreams can cache or compare probes.

**U2 implementation note (2026-10-20):**

- New `crates/local` (`tv-rs-local`): Windows WMI + `EnumDisplayDevicesW`, Linux
  DRM/EDID, macOS/other empty (declared unsupported, caller falls back).
- `plugin-display/src/hardware/probe.rs` is now a 10-line re-export; `wmi` and
  `windows-sys` left the plugin's manifest.
- `cargo test -p tv-rs-local`: 8 unit + 1 doctest pass; `cargo clippy` clean;
  `cargo package --allow-dirty` verifies.
- App tauri-free harness: 8/8 link tests pass after the re-export.
- `tv-rs-local 0.2.0` is published: use the semver dependency
  (`tv-rs-local = "0.2"`), not a sibling path. The temporary path override is gone.

**App adapter:** `plugin-display/src/hardware/link.rs` maps `ProbeInfo` into `DisplayLinkStatus`; the app keeps `display_id`, binding and `TvAssignment`.

**Acceptance:**

- [x] The existing 7 probe tests move and pass in `tv-rs-local`; one lookup test added.
- [ ] Windows reports a real HDMI TV and a non-TV internal panel correctly.
- [ ] Linux behavior is parity or explicitly marked unsupported.
- [x] `tv-rs-core` remains `forbid(unsafe_code)`; platform APIs are isolated in `tv-rs-local`.
- [x] `plugin-display` no longer owns WMI/DRM/EDID parsing.
- [x] App `display_id` and binding semantics are unchanged (8/8 harness tests).

**Risk:** macOS has no implementation today. Either add IOKit in the new crate or declare macOS unsupported; do not silently return "not a TV".

---

### U3 - Move DLNA transport to `tv-rs-dlna`

**Current source:** `plugin-dlna/src/{discovery,soap,gena,gena_client,gena_parse,metadata,models}.rs`.

**Target:** new `tv-rs-dlna` crate, feature `transport`.

**Proposed API:**

```rust
pub struct Renderer {
    pub udn: String,
    pub friendly_name: String,
    pub location: String,
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub services: Vec<Service>,
}

pub async fn discover_renderers(timeout: Duration) -> Result<Vec<Renderer>, DlnaError>;

#[async_trait]
pub trait AvTransport {
    async fn set_uri(&self, uri: &str, metadata: Option<&str>) -> Result<(), DlnaError>;
    async fn play(&self) -> Result<(), DlnaError>;
    async fn pause(&self) -> Result<(), DlnaError>;
    async fn stop(&self) -> Result<(), DlnaError>;
    async fn seek(&self, position: Duration) -> Result<(), DlnaError>;
}

pub struct Subscription { /* SID, timeout, callback */ }
pub async fn subscribe(renderer: &Renderer, callback: &str) -> Result<Subscription, DlnaError>;
```

**Keep in app:** the whole media library and media server — `dlna_media_server.rs`, `plugin-dlna/src/advertise.rs`, `MediaItem`, ContentDirectory, thumbnails, 206 ranges, content auth and `DlnaManager` playlist/caching policy. `tv-rs-dlna` receives only a URI/metadata for transport; it never sees the library.

**Acceptance:**

- [ ] Renderer discovery, cast/stop/playlist and GENA subscribe/poll parity with today.
- [ ] The app's `plugin-dlna` contains no AVTransport/GENA control-point implementation after migration.
- [ ] `tv-rs-dlna` has no Tauri, `sqlx`, `MediaItem` or media-library dependency.
- [ ] Existing `dlna_*` IPC names and payloads remain unchanged.
- [ ] `dlna_media_server.rs` and `advertise.rs` remain app-owned and untouched.

**Fixed boundary:** there is **no media library in `tv-rs`**. `tv-rs-dlna` is a transport/control crate, not a content server. The Signage media library and its HTTP/UPnP server stay in this app.

---

### U4 - Unify health and discovery sources

**Current source:** `src-tauri/src/network/health.rs`, `lan_display_discovery.rs`, `network/mdns.rs`, `network/discovery.rs`.

**Target:**

- `tv-rs-power`: pure `probe_tcp` / `probe_http` reachability and state classification.
- `tv-rs-discovery`: SSDP today; add a generic mDNS `DiscoverySource` when Cast/AirPlay work starts.
- App: DB schema, history, uptime, event bus, `_signagemaster._tcp` advertiser and CMS player discovery.

**Acceptance:**

- [ ] The app no longer has its own TCP/HTTP probe loop for TV reachability.
- [ ] Health history and thresholds remain app-owned and unchanged.
- [ ] `lan_display_discovery` calls `tv-rs` sources but keeps the app union/identity projection.
- [ ] `network/mdns.rs` and `network/discovery.rs` stay app-specific; they are not TV discovery.
- [ ] Cast/AirPlay mDNS is not claimed until a generic browse implementation exists.

---

### U5 - Delete duplicates, publish, and lock the boundary

**Deletion list:**

- `plugin-samsung-remote/src/{remote,tokens,discovery}.rs` protocol bodies; keep only a thin command adapter if the IPC names must remain.
- `plugin-dlna/src/{discovery,soap,gena,gena_client,gena_parse,metadata,models}.rs` after `tv-rs-dlna` parity.
- `plugin-display/src/hardware/probe.rs` platform code after `tv-rs-local` parity.
- `network/health.rs` probe logic after `tv-rs-power` parity.

**Keep (do not move):**

- `src-tauri/src/network/dlna_media_server.rs`, `plugin-dlna/src/advertise.rs`, `plugin-dlna/src/manager.rs`, and all `MediaItem`/content code. These serve the Signage media library.
- `plugin-display/src/hardware/link.rs`, `link_commands.rs`, `tv_assignment.rs`.
- `plugin-player/*`, `plugin-tv-wrapper/*`, player/TV board/TLS sync cert.
- `network/mdns.rs`, `network/discovery.rs`, app health history/DB.
- All UI, scheduling, content, tenancy and licensing code.

**Acceptance:**

- [ ] `rg "danger_client_config|AcceptAnyTvCert" src src-tauri` returns zero.
- [ ] `rg "TizenConnectionPool|SamsungTokenStore" src-tauri --glob '!plugin-samsung-remote/**'` shows only the thin adapter.
- [ ] `rg "SetAVTransportURI|AVTransport|GENA" src-tauri/crates/plugin-dlna` returns zero after parity.
- [ ] `cargo tree` shows one TLS stack and no duplicate Tizen transport.
- [ ] `dlna_media_server.rs` and `advertise.rs` still own the app media server; no `MediaLibrary` type appears in `tv-rs`.
- [ ] Contract checker, typecheck, targeted tests and cargo check are green.
- [ ] `tv-rs` crates are published at a compatible semver; no git or path dependency remains.
- [ ] `tv-rs` has standalone docs/examples and tests that run without Signage; no moved module is a raw copy.
- [ ] The app keeps only adapters/projections for TV behavior.

---

## 6. Migration sequence

| Step | Workstream           | Depends on | User-visible change                    | Roadmap phase    |
| ---- | -------------------- | ---------- | -------------------------------------- | ---------------- |
| 1    | U0 baseline          | -          | none                                   | pre-D3           |
| 2    | U1 Tizen replacement | U0         | pairing/control parity; no UI change   | D3a-D3d          |
| 3    | U2 local probe       | U0         | none; probe moves crate                | D0/D2c follow-up |
| 4    | U3 DLNA transport    | U0         | same cast behavior, one transport impl | D7               |
| 5    | U4 health/discovery  | U0         | same reachability, one probe           | D2c/D7           |
| 6    | U5 cleanup/publish   | U1-U4      | none; dependency and docs cleanup      | D8               |

**Rule:** do not start a workstream until the previous workstream's acceptance criteria are green. U2/U4 can run in parallel with U1 only if the file ownership does not overlap. **U3 never touches the media library or media server**; it only replaces the DLNA transport implementation.

---

## 7. Contracts

### 7.1 Identity

- `display_id` (app) is the primary key.
- `DiscoveredTv { vendor, ip, location, friendly_name, manufacturer, model_name, serial_number, usn }` is evidence.
- `TvClient::id()` is a host string; never persist it as a device id.
- MAC/ARP is optional evidence for Wake; it is not a preference and not a display id.

### 7.2 Capabilities

- Source: `tv_rs_core::CapabilitySet`.
- App projection: `TvCapabilities` (`power`, `volume`, `mute`, `input`, `apps`, `keys`, `pairing_required`, `reason`).
- `pairing_required` and `reason` are app/auth state, not vendor capabilities.
- Runtime `Unsupported` wins over the static set.

### 7.3 Errors and remedies

- `DeviceHandle` methods return `tv_rs_core::TvError` with `remedy()`.
- Facade inherent methods return aggregate `tv_rs::TvError`; map once to the app's `LanState` / action copy.
- Do not create a second TV error enum in `plugin-display`.

### 7.4 Credentials

- Tizen: `SamsungTokenStore`, keyed by `display_id`; migrate legacy IP keys.
- webOS: `save_client_key` / `load_client_key`, keyed by `display_id`.
- Roku: no pairing, no TLS.
- TLS: TOFU first, then `TlsPolicy::pinned(fingerprint)`; `AcceptInvalid` is banned.
- Storage: OS keyring preferred; otherwise app-config file with restrictive ACL. Never in link DTOs, events or logs.

### 7.5 Media

- **The app owns the media library.** `MediaItem`, `DatabaseManager`, thumbnails, content auth, ContentDirectory, 206 ranges, `dlna_media_server.rs` and `plugin-dlna/advertise.rs` stay in Signage.
- **`tv-rs-dlna` owns DLNA transport only:** renderer discovery, AVTransport SOAP and GENA. It receives a URI/metadata; it never receives a library, content root or database handle.
- There is **no `MediaLibrary` trait in `tv-rs`** and no `tv-rs-media` dependency in this program.
- The app's `DlnaManager` keeps playlist/caching/retry policy; the transport crate must not encode signage semantics.

---

## 8. Security requirements

- No permissive TLS in shipped code. Delete `AcceptAnyTvCert` and `danger_client_config`; do not port them upstream.
- Pin by fingerprint, not by "accept anything"; rotate with a visible diagnostic event.
- Tokens/client keys are bearer credentials: redact in logs, never serialize into `DisplayLinkStatus`.
- SSDP/M-SEARCH responses are parsed with bounded sizes; no unbounded XML entity expansion.
- Media paths are confined to the library root; reject traversal (`..`) and absolute paths.
- `tv-rs-local` platform `unsafe` is isolated, documented and tested; no `unsafe` in `tv-rs-core`.
- Dependency review: MIT/Apache compatibility, MSRV, `native-tls` absence, duplicate WebSocket/TLS stacks.

---

## 9. Verification matrix

| Gate           | Command / evidence                                                |
| -------------- | ----------------------------------------------------------------- |
| Contract       | `node tools/checks/check-command-contract.mjs`                    |
| TS             | `bun run typecheck`                                               |
| Display tests  | `cargo test -p plugin-display --lib` (or the tauri-free harness)  |
| TV adapter     | `cargo test -p tv-rs-tizen` / `tv-rs-webos` / `tv-rs-roku`        |
| Local probe    | `cargo test -p tv-rs-local`                                       |
| App media      | App-owned `dlna_media_server` tests; external DLNA browse/play    |
| DLNA transport | `cargo test -p tv-rs-dlna`; cast/stop/GENA parity                 |
| Dependency     | `cargo tree -i native-tls` empty; `cargo tree -d` reviewed        |
| Boundary       | grep checks for retired symbols and app-only dependencies         |
| Hardware       | app matrix: one Tizen, one webOS, one Roku; "unverified" explicit |

---

## 10. Open decisions

1. **Upstream ownership/license.** Confirm the extracted code can be relicensed under MIT and contributed to the public `tv-rs` repo.
2. **`tv-rs-local` name.** `tv-rs-local`, `tv-rs-display`, or `tv-rs-hardware`; choose one before creating the crate.
3. **`tv-rs-dlna` scope.** Transport/control point only. No media library, no ContentDirectory, no HTTP file serving, no database. If a server-side GENA responder is ever needed, it is a separate decision and crate.
4. **Raw Tizen keys.** Add `send_key_raw` upstream, or restrict the UI to the `TizenRemoteKey` enum. The current app accepts arbitrary `KEY_*` values.
5. **Stable device id.** Add a `DeviceId` newtype to `tv-rs-core`, or keep identity entirely app-side. Do not use `TvClient::id()` either way.
6. **Credential abstraction.** Keep vendor-specific stores, or add a generic `PairingStore` trait in `tv-rs-core`. Start vendor-specific; generalize only if both Tizen and webOS need the same shape.
7. **mDNS scope.** The current app mDNS is `_signagemaster._tcp`, not Cast/AirPlay. Only upstream a generic browser when D7 starts.
8. **Agent adoption.** Do not adopt `tv-rs-agent` in this program. Revisit after U5 if the app wants fleet reconciliation.
9. **Publication cadence.** Everything is on crates.io at `0.2.0` — the facade, the fourteen library crates and the two the app needed (`tv-rs-local`, `tv-rs-dlna`). Use semver, not a git pin. Do not publish a media library crate.

---

## 11. Acceptance checklist

- [ ] `tv-rs` is the only implementation of TV SSDP/mDNS, vendor control, pairing, TLS and sessions.
- [ ] The Signage media library and media server remain app-owned; `tv-rs` has no `MediaLibrary`, `MediaItem`, ContentDirectory or 206 server.
- [ ] `tv-rs-dlna` owns DLNA transport (renderer discovery/AVTransport/GENA); the app has only playlist/caching orchestration.
- [ ] `tv-rs-local` owns EDID/connector probing; app has only a projection.
- [ ] `plugin-samsung-remote` has no protocol body and no permissive TLS.
- [ ] `plugin-dlna` contains no DLNA transport implementation after migration; `dlna_media_server.rs` and `advertise.rs` remain app-owned.
- [ ] `TvCapabilities` derives from `CapabilitySet`; no second capability source.
- [ ] `TvError::remedy()` is mapped once; no second TV error enum.
- [ ] All retired symbols are absent from the app.
- [ ] Contract/type/cargo/hardware gates are green.
- [ ] The roadmap and this spec both point to the same sequence and ownership table.
