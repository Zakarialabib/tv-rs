# Architecture

The workspace is fourteen published crates, a reference CLI and a task runner,
arranged in four layers. The layering is a property of the dependency graph, not of
folders — every package lives at `crates/<dir>` and the shape below is what makes the
graph valid.

## Layers

| Layer | Crates | Paths | Responsibility |
|---|---|---|---|
| **Device model** | `tv-rs-core` | `crates/core` | capabilities, `DeviceHandle`, `CommandId`, confidence-tagged state, the error taxonomy, the mock backend, the conformance suite, the quirks table. **No tokio, in any configuration.** |
| **Facade** | `tv-rs` | `crates/tv-rs` | the only multi-vendor-aware code: discovery orchestration, `identify_vendor`, `TvClient` dispatch (which implements `DeviceHandle`), quirk-driven port pinning |
| **Protocol** | `tv-rs-discovery`, `tv-rs-power`, `tv-rs-tizen`, `tv-rs-webos`, `tv-rs-roku`, `tv-rs-mdc`, `tv-rs-cec` | `crates/{discovery,power,tizen,webos,roku,mdc,cec}` | one wire protocol each: SSDP, Wake-on-LAN, Tizen remote channel, LG SSAP, Roku ECP, Samsung MDC over TCP 1515, and HDMI-CEC over `cec-client` |
| **Support** | `tv-rs-transport`, `tv-rs-media` | `crates/{transport,media}` | shared plumbing: TLS policy + WebSocket frame reader; an independent HTTP `206`/HLS server |
| **Reconciliation** | `tv-rs-agent` | `crates/agent` | desired state in, fleet converged: providers, `decide()`, the healer ladder, the graduated remediation ladder (`remedy`), the JSONL audit log, the loop. Depends on `core` only |
| **Tooling** | `tv-rs-cli`, `xtask` | `crates/cli`, `xtask` | reference consumer (`publish = false`), workspace automation (`publish = false`) |

`discovery` and `power` are **protocol** crates, not support: SSDP and Wake-on-LAN
are wire protocols with their own request/response shapes. A folder called `core/`
used to suggest otherwise and no longer exists (see
[ADR-002](adr/002-workspace-layout.md)).

## Dependency rule

```
            ┌───────────────────────────┐        ┌──────────────────────┐
            │  tv-rs  (facade)          │◀───────│ tv-rs-agent          │
            └──┬───────┬───────┬────────┘        │ decide() + loop      │
               │       │       │                 └──────────┬───────────┘
     ┌─────────▼──┐ ┌──▼─────┐ ┌▼────────┐                   │
     │ tizen      │ │ webos  │ │ roku    │  protocol crates   │
     └─────┬──────┘ └───┬────┘ └────┬────┘                   │
           │            │           │                        │
     ┌─────▼────────────▼───────────▼────┐                   │
     │ transport (TLS, frame reader)     │                   │
     └───────────────────┬───────────────┘                   │
                         ▼                                   ▼
     ┌────────────────────────────────────────────────────────────┐
     │ tv-rs-core: capabilities, DeviceHandle, state, errors       │
     │ no tokio — the CI gate fails the build if that changes      │
     └────────────────────────────────────────────────────────────┘

  discovery, power  ── no dependency on any vendor crate
  mdc               ── TCP 1515 frames; depends on core, not on transport
  cec               ── shells out to cec-client; no workspace dependency
  media             ── depends on nothing in the workspace (pulls axum)
```

1. **Protocol crates never depend on each other.** Tizen does not know webOS
   exists; a cross-vendor operation belongs in the facade.
2. **Only the facade may match on vendor.** If a helper needs `match vendor`, it
   belongs in `crates/tv-rs/src/client.rs`, not in a protocol crate.
3. **`transport` is the only shared crate** the vendor crates link, and it carries
   no vendor vocabulary.
4. **`media` must never appear in a protocol crate's graph.** It pulls `axum`; a
   control crate that pulled `axum` would ship a web server to consumers who only
   wanted a remote control.
5. **`core` is tokio-free in every configuration**, and CI checks the graph rather
   than the intent (`cargo tree -p tv-rs-core`). An application that wants only the
   device model — a CMS building its own fleet layer — gets no runtime.
6. **`cec` does not link libcec.** It spawns `cec-client`, which keeps `unsafe` and a
   C toolchain out of the workspace; a missing binary is a typed `Unavailable`, not a
   build failure.

## Module inventory

Every crate uses the same file roles, and a role appears only when the protocol
has that concept.

| Crate | `client` | `protocol` | `payload` | `types` | `error` | `auth` |
|---|---|---|---|---|---|---|
| `tv-rs` (facade) | ✓ dispatch | — | — | ✓ vendor/discovery types | ✓ `TvError` | — |
| `discovery` | ✓ scanner | ✓ `SsdpDiscovery` | ✓ SSDP/UPnP | ✓ ports/constants | ✓ | — |
| `power` | ✓ WoL + probe | ✓ `PowerControl` | ✓ magic packet | ✓ `MacAddress`, `PowerState` | ✓ | — |
| `tizen` | ✓ channel/session | ✓ `WebSocketHandshake`, `DialLauncher` | ✓ channel frames | ✓ keys/ports | ✓ | ✓ pairing token |
| `webos` | ✓ SSAP session | ✓ `SsapController` | ✓ JSON-RPC frames | ✓ `SsapUri` | ✓ | ✓ `client-key` |
| `roku` | ✓ ECP REST | ✓ `EcpRestClient` | ✓ XML views | ✓ `RokuKey`, paths | ✓ | — |
| `mdc` | ✓ `MdcClient` + `MdcDevice` | ✓ frame codec + checksum | ✓ `Status` | ✓ `DisplayId`, `InputSource`, `MdcPower` | ✓ `MdcError` | — |
| `transport` | — | — | — | ✓ `TlsPolicy` | ✓ `Unsupported` | — |
| `media` | — | — | — | ✓ ranges/segments | — | — |
| `core` | — | — | — | ✓ ids, capabilities, state, `Telemetry` | ✓ `TvError` + `Remedy` | — |
| `agent` | — | ✓ telemetry policy (`assess_telemetry`) + remediation ladder (`plan_remedy`) | ✓ desired-state JSON | ✓ `Decision`, `Outcome`, `TelemetryVerdict`, `RemedyStep` | ✓ provider errors | — |
| `cec` | ✓ `CecClient` (in `lib.rs`) | ✓ `cec-client` grammar | — | ✓ `CecAddress` | ✓ | — |

**The auth rule.** An `auth.rs` exists only where the protocol performs a real
credential handshake: Tizen issues a pairing token, webOS returns a `client-key`.
Discovery, power and Roku authenticate nothing, so they have no auth module — a
marker trait that always returns "no credentials" is noise, and Roku's was removed
for exactly that reason.

Additional files that exist where a protocol needs them, not by convention:
`tizen/token_store.rs` (pairing persistence), `tizen/pool.rs` (one channel per TV),
`tizen/discovery.rs` (Samsung's private SSDP target), `tizen/apps.rs` (app-id
presets), `media/{range,hls,server}.rs` (the three things a TV player demands),
`core/{mock,conformance,quirks,telemetry}.rs` (tests, the quirks table, and what a
panel reports about its own health), `agent/{desired,healer,audit,reconciler,runner,telemetry,remedy}.rs` (the loop and its pure core), `cec/transport.rs` (the `cec-client` invocation).

## Directory naming

- **Directories are unprefixed, packages keep `tv-rs-`**: `crates/tizen` is the
  package `tv-rs-tizen`. The facade is the one exception — `crates/tv-rs` is the
  package `tv-rs`.
- **A crate name is used in prose, a path when pointing at code.** "`tizen`" and
  "`tv-rs-tizen`" are the same crate; `crates/tizen/src/client.rs` is a file.
- **Test-only crates are `publish = false`** (`cli`, `xtask`).

## Where the transport crate sits

`tv-rs-transport` has two jobs and one reason to exist:

1. **Plumbing.** One TLS decision for both vendor crates — `rustls` (default) or
   `native-tls` (opt-in, mutually exclusive by a `compile_error!`), plus the
   budgeted frame reader that stops a silent TV from hanging a caller.
2. **Shared vocabulary.** The `Unsupported` error that every crate returns for a
   deferred capability. Consumers that only need that vocabulary depend on it with
   `default-features = false`, so no TLS stack enters their graph.

If the shared vocabulary ever grows beyond errors and its feature story, split it
into its own crate rather than letting `transport` become a grab bag.

## Playbook → crate map

The protocol facts live in [`01-playbook.md`](01-playbook.md); this is which part
of them each crate implements.

| Playbook section | Crate | State |
|---|---|---|
| §1 SSDP discovery + UPnP descriptor | `tv-rs-discovery` | implemented (mDNS and ARP are *planned*) |
| §2 Wake-on-LAN + power probing | `tv-rs-power` | implemented (WoWLAN reasons planned) |
| §3 Samsung Tizen channel + DIAL | `tv-rs-tizen` | implemented to Phase 1; TLS handled by pinning (`Pinned`/`TrustOnFirstUse`), enterprise token math and AES-GCM framing deferred |
| §4 LG webOS SSAP | `tv-rs-webos` | implemented to Phase 1; 3001 TLS handled by pinning, richer `SsapUri` coverage deferred |
| §5 Roku ECP | `tv-rs-roku` | implemented in full (ECP is unauthenticated) |
| §6 Android TV ADB | — | not started |
| §7–8 DLNA control, `206`, HLS | `tv-rs-media` (serving only) + not started (SOAP control) | range + HLS implemented; DLNA `AVTransport`/`RenderingControl` not started |
| §9 Google Cast / AirPlay | — | not started |
| HDMI-CEC (no playbook section) | `tv-rs-cec` | power on, standby, active source, over `cec-client` |
| MDC (no playbook section) | `tv-rs-mdc` | commercial Samsung panels: power, input, volume/mute, brightness, status; display-ID addressing over one socket per wall |
| §10 undocumented auth | `tizen`, `webos` | deferred, surfaced as typed `Unsupported` errors |