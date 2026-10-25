# Architecture

The workspace is fifteen publishable crates plus a reference CLI and a task runner,
arranged in the layers below. The layering is a property of the dependency graph, not of
folders — every package lives at `crates/<dir>` and the shape below is what makes the
graph valid. Thirteen of the fifteen are on crates.io at `0.2.0`; `tv-rs-mdc` and
`tv-rs-app` are merged here and their first publish rides the `0.3.0` release (registry
table in [`03-matrix.md`](03-matrix.md#published-cratesio-verified-2026-10-22-against-the-registry-api)).

## Layers

| Layer              | Crates                                                                                                 | Paths                                               | Responsibility                                                                                                                                                                                                                |
| ------------------ | ------------------------------------------------------------------------------------------------------ | --------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Device model**   | `tv-rs-core`                                                                                           | `crates/core`                                       | capabilities (incl. the commercial LFD subset), `DeviceHandle`, `CommandId`, confidence-tagged state, the error taxonomy, the mock backend, the conformance suite, the quirks table. **No tokio, in any configuration.**      |
| **Facade**         | `tv-rs`                                                                                                | `crates/tv-rs`                                      | the only multi-vendor-aware code: discovery orchestration, `identify_vendor`, `TvClient` dispatch (which implements `DeviceHandle`), quirk-driven port pinning                                                                |
| **Protocol**       | `tv-rs-discovery`, `tv-rs-power`, `tv-rs-tizen`, `tv-rs-webos`, `tv-rs-roku`, `tv-rs-mdc`, `tv-rs-cec` | `crates/{discovery,power,tizen,webos,roku,mdc,cec}` | one wire protocol each: SSDP, Wake-on-LAN, Tizen remote channel, LG SSAP, Roku ECP, Samsung MDC over TCP 1515 or RS-232, and HDMI-CEC over `cec-client`                                                                       |
| **Support**        | `tv-rs-transport`, `tv-rs-dlna`, `tv-rs-media`, `tv-rs-local`                                          | `crates/{transport,dlna,media,local}`               | shared plumbing and host-side transports: TLS policy + WebSocket frame reader; DLNA MediaRenderer transport (SSDP + AVTransport SOAP + GENA); an independent HTTP `206`/HLS server; the host's own display probe (EDID/modes) |
| **App producer**   | `tv-rs-app`                                                                                            | `crates/app`                                        | a supplied web/wasm payload in, an installable TV app out: Tizen `.wgt`, webOS app directory, web bundle, manifests, D-pad key mapping. Depends on nothing in the workspace                                                   |
| **Reconciliation** | `tv-rs-agent`                                                                                          | `crates/agent`                                      | desired state in, fleet converged: providers, `decide()`, the healer ladder, the graduated remediation ladder (`remedy`), the JSONL audit log, the loop. Depends on `core` only                                               |
| **Tooling**        | `tv-rs-cli`, `xtask`                                                                                   | `crates/cli`, `xtask`                               | reference consumer (`publish = false`), workspace automation (`publish = false`)                                                                                                                                              |

`discovery` and `power` are **protocol** crates, not support: SSDP and Wake-on-LAN
are wire protocols with their own request/response shapes. A folder called `core/`
used to suggest otherwise and no longer exists — that history is in
[§Workspace layout](#workspace-layout).

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
  mdc               ── TCP 1515 or RS-232 frames; depends on core, not on transport
  local             ── host-side EDID/mode probe; no workspace dependency
  cec               ── shells out to cec-client; no workspace dependency
  dlna              ── SSDP + SOAP + GENA renderer transport; no workspace dependency
  media             ── depends on nothing in the workspace (pulls axum)
  app               ── depends on nothing in the workspace
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

| Crate            | `client`                    | `protocol`                                                                   | `payload`                 | `types`                                                                   | `error`                | `auth`          |
| ---------------- | --------------------------- | ---------------------------------------------------------------------------- | ------------------------- | ------------------------------------------------------------------------- | ---------------------- | --------------- |
| `tv-rs` (facade) | ✓ dispatch                  | —                                                                            | —                         | ✓ vendor/discovery types                                                  | ✓ `TvError`            | —               |
| `discovery`      | ✓ scanner                   | ✓ `SsdpDiscovery`                                                            | ✓ SSDP/UPnP               | ✓ ports/constants                                                         | ✓                      | —               |
| `power`          | ✓ WoL + probe               | ✓ `PowerControl`                                                             | ✓ magic packet            | ✓ `MacAddress`, `PowerState`                                              | ✓                      | —               |
| `tizen`          | ✓ channel/session           | ✓ `WebSocketHandshake`, `DialLauncher`                                       | ✓ channel frames          | ✓ keys/ports                                                              | ✓                      | ✓ pairing token |
| `webos`          | ✓ SSAP session              | ✓ `SsapController`                                                           | ✓ JSON-RPC frames         | ✓ `SsapUri`                                                               | ✓                      | ✓ `client-key`  |
| `roku`           | ✓ ECP REST                  | ✓ `EcpRestClient`                                                            | ✓ XML views               | ✓ `RokuKey`, paths                                                        | ✓                      | —               |
| `mdc`            | ✓ `MdcClient` + `MdcDevice` | ✓ one frame codec + checksum on two carriers (TCP, RS-232) + LFD commands    | ✓ `Status`, `ErrorStatus` | ✓ `DisplayId`, `InputSource`, `MdcOrientation`, `MdcPower`                | ✓ `MdcError`           | —               |
| `transport`      | —                           | —                                                                            | —                         | ✓ `TlsPolicy`                                                             | ✓ `Unsupported`        | —               |
| `media`          | —                           | —                                                                            | —                         | ✓ ranges/segments                                                         | —                      | —               |
| `core`           | —                           | —                                                                            | —                         | ✓ ids, capabilities (incl. the LFD subset), state, `Telemetry`            | ✓ `TvError` + `Remedy` | —               |
| `agent`          | —                           | ✓ telemetry policy (`assess_telemetry`) + remediation ladder (`plan_remedy`) | ✓ desired-state JSON      | ✓ `Decision`, `Outcome`, `TelemetryVerdict`, `RemedyStep`                 | ✓ provider errors      | —               |
| `cec`            | ✓ `CecClient` (in `lib.rs`) | ✓ `cec-client` grammar                                                       | —                         | ✓ `CecAddress`                                                            | ✓                      | —               |
| `local`          | —                           | —                                                                            | —                         | ✓ `ProbeInfo`, `Attachment`, `DisplayEdid` (`lib.rs`); `Edid` (`edid.rs`) | ✓                      | —               |
| `dlna`           | ✓ `SoapAvTransport`         | ✓ SSDP discovery/GENA request builders                                       | ✓ DIDL-Lite               | ✓ `Renderer`                                                              | ✓ `DlnaError`          | —               |
| `app`            | —                           | ✓ manifest/bundle builders                                                   | ✓ descriptor              | ✓ `Target`, key map                                                       | ✓ `AppError`           | —               |

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
panel reports about its own health), `agent/{desired,healer,audit,reconciler,runner,telemetry,remedy}.rs` (the loop and its pure core), `cec/transport.rs` (the `cec-client` invocation),
`dlna/{soap,gena,gena_client,gena_parse,gena_registry,advertise,metadata}.rs` (renderer control on the wire and its eventing),
`app/{descriptor,manifest,bundle,zip,bridge,keys,target}.rs` (the TV app producer).

The DLNA crate is the one deliberate departure from the fixed module vocabulary:
its domain names (`soap.rs`, `gena*.rs`, `advertise.rs`) are the UPnP service
names, and renaming them to `client.rs`/`protocol.rs` would erase the mapping the
specification imposes.

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

The protocol facts live in [`02-playbook.md`](02-playbook.md); this is which part
of them each crate implements.

| Playbook section                    | Crate             | State                                                                                                                                                                                              |
| ----------------------------------- | ----------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| §1 SSDP discovery + UPnP descriptor | `tv-rs-discovery` | implemented (mDNS and ARP are _planned_)                                                                                                                                                           |
| §2 Wake-on-LAN + power probing      | `tv-rs-power`     | implemented (WoWLAN reasons planned)                                                                                                                                                               |
| §3 Samsung Tizen channel + DIAL     | `tv-rs-tizen`     | implemented; TLS handled by pinning (`Pinned`/`TrustOnFirstUse`), enterprise token math and AES-GCM framing deferred                                                                               |
| §4 LG webOS SSAP                    | `tv-rs-webos`     | implemented; 3001 TLS handled by pinning, richer `SsapUri` coverage deferred                                                                                                                       |
| §5 Roku ECP                         | `tv-rs-roku`      | implemented in full (ECP is unauthenticated)                                                                                                                                                       |
| §6 Samsung MDC                      | `tv-rs-mdc`       | commercial Samsung panels: power, input, volume/mute, brightness, status, the LFD subset; display-ID addressing over one socket per wall, on TCP 1515 or RS-232                                    |
| §7 HDMI-CEC                         | `tv-rs-cec`       | power on, standby, active source, over `cec-client`                                                                                                                                                |
| §8 DLNA renderer control            | `tv-rs-dlna`      | implemented: SSDP discovery, AVTransport SOAP (`SetAVTransportURI`/`Play`/`Pause`/`Stop`/`Seek`), GENA; advertisement for a caller-hosted MediaServer; NOT a media library or ContentDirectory     |
| §9 `206` + HLS serving              | `tv-rs-media`     | range + HLS implemented; HLS segmentation (cutting segments) not started                                                                                                                           |
| §10 deferred surfaces               | —                 | not implemented: pinning verification on hardware, Samsung enterprise token math, AES-GCM framing, Cast/AirPlay/Android TV — the list is [02-playbook §10](02-playbook.md#10-not-implemented-here) |
| Host display probe (unnumbered)     | `tv-rs-local`     | EDID + connector/mode probe; Windows WMI + registry EDID, Linux DRM, macOS declared unsupported                                                                                                    |

## Workspace layout

How the tree is arranged, and why — the decision that produced the current shape.

**Flat `crates/` tree.** Every package lives at `crates/<dir>`; directories are
unprefixed, packages keep the `tv-rs-` prefix. The facade is `crates/tv-rs` → package
`tv-rs`. An earlier tree had three code homes (`src/` for the facade, `core/` for
discovery + power + transport, `crates/` for the vendor crates and the media server) and
two misleading folder names: `core/` grouped protocol crates (SSDP, Wake-on-LAN) with a
support crate (transport), and `crates/` mixed vendor crates with the media server, so
"in `crates/`" said nothing about what a crate was.

**Virtual workspace root.** the root `Cargo.toml` has no `[package]`; members are listed
**explicitly** (no glob). A glob that stops matching (an emptied `crates/*`) fails the
whole build with `failed to read crates\*\Cargo.toml`, which is a confusing way to learn
that a directory moved.

**Layering is documented, not imposed by folders.** The facade/protocol/support model and
the "protocol crates depend only on transport" rule live in this page; the dependency
graph enforces them, and a parallel folder split would have been a second, weaker copy of
the same idea.

**`missing_docs = "deny"` lives in `[workspace.lints.rust]`**, inherited through
`[lints] workspace = true`, so a new crate cannot forget it. See
[05-rust-package-standard §Crate Root Requirements](05-rust-package-standard.md).

**Tooling is `xtask`.** `cargo xtask publish` replaces a shell script; the publish order
lives next to the code it describes, and `cargo` can verify each package still exists.

Rejected: `crates/protocols/` + `crates/support/` subfolders (truthful, but two-level
globs and a deviation from the near-universal Rust convention for no gain); keeping the
facade as a root package (three code homes and a misleading bare `cargo build`); keeping
the shell release script (a script cannot know whether `tv-rs-tizen` still exists).

## Publishing

crates.io facts that shape the release path, and the sequence they produce.

- **Yank, not delete.** A published version can never be removed or corrected in
  place — only a new version fixes it.
- **There is no stable `cargo publish --workspace`.** Multi-package publication is
  not atomic, and a **single root-level `cargo publish` publishes only the root
  package** — whose path dependencies have just become unsatisfiable registry
  requirements. This exact defect was present in this workspace's release workflow and
  would have failed on first use.
- **Path dependencies are stripped at publish time**, so each carries a `version` that
  resolves to a real registry entry.

The order lives in one place, `xtask/src/main.rs::ORDER`, so it cannot drift from the
manifests:

```
tv-rs-core → tv-rs-transport → tv-rs-discovery → tv-rs-power → tv-rs-local
  → tv-rs-dlna → tv-rs-roku → tv-rs-mdc → tv-rs-tizen → tv-rs-webos → tv-rs
  → tv-rs-media → tv-rs-agent → tv-rs-cec
```

`tv-rs-core` leads because every backend and the facade depend on its vocabulary;
`tv-rs-transport` follows for the crates that open a TLS channel. `tv-rs-local`,
`tv-rs-dlna`, `tv-rs-media`, `tv-rs-agent` and `tv-rs-cec` are independent leaves, and
the facade (`tv-rs`) comes after the protocol crates it names. `tv-rs-app` is likewise a
leaf. The full rules and the pre-release checklist are
[05-rust-package-standard §Publishing](05-rust-package-standard.md).
