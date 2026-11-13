# Architecture

The workspace is **nineteen published crates** plus a reference CLI, an Android TV
spike, ten deprecated `publish = false` shims, and a task runner, arranged in the
layers below. The layering is a property of the dependency graph, not of folders —
every package lives at `crates/<dir>` and the shape below is what makes the graph
valid. All nineteen are on crates.io at `0.8.0` (registry table in
[`03-matrix.md`](03-matrix.md)).

## Layers

| Layer                        | Crates                                                                                                                            | Paths                                                                 | Responsibility                                                                                                                                                                                                                                                                                                                                    |
| ---------------------------- | --------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Layer                        | Crates                                                                                                                            | Paths                                                                 | Responsibility                                                                                                                                                                                                                                                                                                                                    |
| **Device model**             | `tv-rs-core`                                                                                                                      | `crates/core`                                                         | capabilities (incl. the commercial LFD subset), `DeviceHandle`, `CommandId`, confidence-tagged state, the error taxonomy, the mock backend, the conformance suite, the quirks table. **No tokio, in any configuration.**                                                                                                                          |
| **Facade**                   | `tv-rs`                                                                                                                           | `crates/tv-rs`                                                        | the only multi-vendor-aware code: discovery orchestration, `identify_vendor`, `TvClient` dispatch (which implements `DeviceHandle`), quirk-driven port pinning                                                                                                                                                                                    |
| **Protocol**                 | `tv-rs-drivers`, `tv-rs-lgcmd`, `tv-rs-bluetooth`                                                                                 | `crates/{drivers,lgcmd,bluetooth}`                                    | one wire protocol/emulation each: SSDP, Wake-on-LAN, Tizen remote channel, LG SSAP, Roku ECP, Google Cast sender, Samsung MDC over TCP 1515 or RS-232, Philips SICP, HDMI-CEC (subprocess/native FFI) and IR behind `tv-rs-drivers` feature flags; LG commercial serial in `tv-rs-lgcmd`; Bluetooth HID/BLE remote emulation in `tv-rs-bluetooth` |
| **Deprecated shims**         | the ten `tv-rs-<vendor>` crates                                                                                                   | `crates/{cast,cec,discovery,ir,mdc,power,roku,sicp,tizen,webos}`      | `publish = false` and never on crates.io: a one-line glob re-export of `tv-rs-drivers` with a `#[deprecated]` note, kept only so in-repo references still compile. A published crate must never depend on one                                                                                                                                     |
| **Support**                  | `tv-rs-transport`, `tv-rs-dlna`, `tv-rs-media`, `tv-rs-local`, `tv-rs-vision`, `tv-rs-mirror`, `tv-rs-spatial-audio`, `tv-rs-sim` | `crates/{transport,dlna,media,local,vision,mirror,spatial-audio,sim}` | shared plumbing and host-side transports: TLS policy + WebSocket frame reader; DLNA MediaRenderer transport (SSDP + AVTransport SOAP + GENA); an independent HTTP `206`/HLS server; host display probe; RTSP computer vision & edge AI events; video mirroring vocabulary; spatial audio orientation; mock hardware simulator                     |
| **Grounding & Rules**        | `tv-rs-rag`, `tv-rs-rules`                                                                                                        | `crates/{rag,rules}`                                                  | in-memory RAG grounding (vendor quirks corpus, episodic remediation ledger, rule AST templates) and declarative rule engine                                                                                                                                                                                                                       |
| **Companion & Provisioning** | `tv-rs-companion`, `tv-rs-provision`                                                                                              | `crates/{companion,provision}`                                        | mobile companion core (no_std+alloc, UniFFI Swift/Kotlin bindings, SAS ceremony) and first-boot provisioning (BLE Improv, QR payload, multicast lock)                                                                                                                                                                                             |
| **App producer**             | `tv-rs-app`                                                                                                                       | `crates/app`                                                          | a supplied web/wasm payload in, an installable TV app out: Tizen `.wgt`, webOS app directory, web bundle, manifests, D-pad key mapping. Depends on nothing in the workspace                                                                                                                                                                       |
| **Reconciliation**           | `tv-rs-agent`                                                                                                                     | `crates/agent`                                                        | desired state in, fleet converged: providers, `decide()`, the healer ladder, the graduated remediation ladder (`remedy`), the JSONL audit log, the loop. Depends on `core` only                                                                                                                                                                   |
| **Tooling**                  | `tv-rs-cli`, `tv-rs-androidtv`, `xtask`                                                                                           | `crates/{cli,androidtv}`, `xtask`                                     | reference consumer (`publish = false`), Android TV v2 spike (`publish = false`), workspace automation (`publish = false`)                                                                                                                                                                                                                         |

`discovery` and `power` are **protocol** modules of `tv-rs-drivers`, not support:
SSDP and Wake-on-LAN are wire protocols with their own request/response shapes. The
standalone crates that used to carry them are deprecated, unpublished shims.

## Dependency rule

```
            ┌───────────────────────────┐        ┌──────────────────────┐
            │  tv-rs  (facade)          │◀───────│ tv-rs-agent          │
            └──┬────────────────────────┘        │ decide() + loop      │
               │                                 └──────────┬───────────┘
               │                                            │
     ┌─────────▼────────────────────────────────────────────▼───┐
     │ tv-rs-drivers (feature flags)                             │
     │ tizen · webos · roku · cast · cec · mdc · sicp · ir ·     │
     │ discovery · power                                         │
     └───────────────────────────┬───────────────────────────────┘
                                 │
     ┌───────────────────────────▼───────────────────────────────┐
     │ transport (TLS, frame reader)                              │
     └───────────────────────────┬───────────────────────────────┘
                                 ▼
     ┌────────────────────────────────────────────────────────────┐
     │ tv-rs-core: capabilities, DeviceHandle, state, errors       │
     │ no tokio — the CI gate fails the build if that changes      │
     └────────────────────────────────────────────────────────────┘

  drivers/tizen, drivers/webos, drivers/roku, drivers/cast ── link transport (TLS, frames)
  drivers/discovery, drivers/power ── link transport for the shared vocabulary only
  drivers/mdc, drivers/sicp, drivers/ir, drivers/cec ── depend on core, not on transport
  lgcmd, bluetooth ── independent of transport and of every vendor module
  local ── host-side EDID/mode probe; no workspace dependency
  dlna ── SSDP + SOAP + GENA renderer transport; no workspace dependency
  media ── depends on nothing in the workspace (pulls axum)
  app ── depends on nothing in the workspace
```

1. **Vendor protocols never depend on each other.** `tv-rs-drivers` compiles each
   behind its own feature, and `tv-rs-lgcmd`/`tv-rs-bluetooth` are standalone. A
   cross-vendor operation belongs in the facade.
2. **Only the facade may match on vendor.** If a helper needs `match vendor`, it
   belongs in `crates/tv-rs/src/client.rs`, not in a protocol module.
3. **`transport` is the only shared crate** the vendor protocols link, and it carries
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

Every protocol module uses the same file roles, and a role appears only when the
protocol has that concept.

| Crate                        | `client`            | `protocol`                                                                   | `payload`            | `types`                                                                   | `error`                | `auth`         |
| ---------------------------- | ------------------- | ---------------------------------------------------------------------------- | -------------------- | ------------------------------------------------------------------------- | ---------------------- | -------------- |
| Crate                        | `client`            | `protocol`                                                                   | `payload`            | `types`                                                                   | `error`                | `auth`         |
| `tv-rs` (facade)             | ✓ dispatch          | —                                                                            | —                    | ✓ vendor/discovery types                                                  | ✓ `TvError`            | —              |
| `tv-rs-drivers` (per vendor) | ✓ per vendor        | ✓ per vendor                                                                 | ✓ per vendor         | ✓ per vendor                                                              | ✓ per vendor           | ✓ tizen, webos |
| `tv-rs-lgcmd`                | ✓ `LgCmdClient`     | ✓ `ka`/`mc` grammar                                                          | —                    | ✓ commands/ports                                                          | ✓ `LgCmdError`         | —              |
| `tv-rs-bluetooth`            | ✓ `BluetoothRemote` | ✓ HID reports                                                                | —                    | ✓ keys/profiles                                                           | ✓ `BluetoothError`     | —              |
| `transport`                  | —                   | —                                                                            | —                    | ✓ `TlsPolicy`                                                             | ✓ `Unsupported`        | —              |
| `media`                      | —                   | —                                                                            | —                    | ✓ ranges/segments                                                         | —                      | —              |
| `core`                       | —                   | —                                                                            | —                    | ✓ ids, capabilities (incl. the LFD subset), state, `Telemetry`            | ✓ `TvError` + `Remedy` | —              |
| `agent`                      | —                   | ✓ telemetry policy (`assess_telemetry`) + remediation ladder (`plan_remedy`) | ✓ desired-state JSON | ✓ `Decision`, `Outcome`, `TelemetryVerdict`, `RemedyStep`                 | ✓ provider errors      | —              |
| `local`                      | —                   | —                                                                            | —                    | ✓ `ProbeInfo`, `Attachment`, `DisplayEdid` (`lib.rs`); `Edid` (`edid.rs`) | ✓                      | —              |
| `dlna`                       | ✓ `SoapAvTransport` | ✓ SSDP discovery/GENA request builders                                       | ✓ DIDL-Lite          | ✓ `Renderer`                                                              | ✓ `DlnaError`          | —              |
| `app`                        | —                   | ✓ manifest/bundle builders                                                   | ✓ descriptor         | ✓ `Target`, key map                                                       | ✓ `AppError`           | —              |
| `vision`                     | —                   | ✓ RTSP stream / inference pipeline                                           | —                    | ✓ `VisionEvent`, `Zone`, `Detection`, `Pose`                              | ✓ `VisionError`        | —              |

**The auth rule.** An `auth.rs` exists only where the protocol performs a real
credential handshake: Tizen issues a pairing token, webOS returns a `client-key`.
Discovery, power and Roku authenticate nothing, so they have no auth module — a
marker trait that always returns "no credentials" is noise, and Roku's was removed
for exactly that reason.

Additional files exist where a protocol needs them, not by convention:
`drivers/src/tizen/{token_store,pool,discovery,apps,remote}.rs` (pairing persistence,
one channel per TV, Samsung's private SSDP target, app-id presets, the remote-key
vocabulary), `drivers/src/webos/{pointer,pool}.rs` (the pointer socket and the
per-TV session pool), `media/{range,hls,server}.rs` (the three things a TV player
demands), `core/{mock,conformance,quirks,telemetry}.rs` (tests, the quirks table, and
what a panel reports about its own health),
`agent/{desired,healer,audit,reconciler,runner,telemetry,remedy}.rs` (the loop and its
pure core), `drivers/src/cec/transport.rs` (the `cec-client` invocation),
`dlna/{soap,gena,gena_client,gena_parse,gena_registry,advertise,metadata}.rs` (renderer
control on the wire and its eventing),
`app/{descriptor,manifest,bundle,zip,bridge,keys,target}.rs` (the TV app producer).

The DLNA crate is the one deliberate departure from the fixed module vocabulary:
its domain names (`soap.rs`, `gena*.rs`, `advertise.rs`) are the UPnP service
names, and renaming them to `client.rs`/`protocol.rs` would erase the mapping the
specification imposes.

## Directory naming

- **Directories are unprefixed, packages keep `tv-rs-`** — with one exception: the
  facade is `crates/tv-rs` → package `tv-rs`.
- **The ten `crates/<vendor>` directories are shims, not protocol crates.**
  `crates/tizen` is the package `tv-rs-tizen` (`publish = false`), a one-line
  re-export of `tv-rs-drivers`. The protocol code is in
  `crates/drivers/src/<vendor>/`; do not add files to a shim.
- **A crate name is used in prose, a path when pointing at code.** "`tizen`" and
  "`tv-rs-drivers` (feature `tizen`)" name the same code; `crates/drivers/src/tizen`
  is a directory.
- **`publish = false` members are workspace-only**: `cli`, `androidtv`, `xtask`, and
  the ten shims. A published crate that depends on one of them cannot ship.

## Where the transport crate sits

`tv-rs-transport` has two jobs and one reason to exist:

1. **Plumbing.** One TLS decision for the vendor protocols — `rustls` (default) or
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

| Playbook section                    | Crate                         | State                                                                                                                                                                                          |
| ----------------------------------- | ----------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Playbook section                    | Crate                         | State                                                                                                                                                                                          |
| §1 SSDP discovery + UPnP descriptor | `tv-rs-drivers` (`discovery`) | implemented (mDNS and ARP are _planned_)                                                                                                                                                       |
| §2 Wake-on-LAN + power probing      | `tv-rs-drivers` (`power`)     | implemented (WoWLAN reasons planned)                                                                                                                                                           |
| §3 Samsung Tizen channel + DIAL     | `tv-rs-drivers` (`tizen`)     | implemented; TLS handled by pinning (`Pinned`/`TrustOnFirstUse`), enterprise token math and AES-GCM framing deferred                                                                           |
| §4 LG webOS SSAP                    | `tv-rs-drivers` (`webos`)     | implemented; 3001 TLS handled by pinning, richer `SsapUri` coverage deferred                                                                                                                   |
| §5 Roku ECP                         | `tv-rs-drivers` (`roku`)      | implemented in full (ECP is unauthenticated)                                                                                                                                                   |
| §6 Samsung MDC                      | `tv-rs-drivers` (`mdc`)       | commercial Samsung panels: power, input, volume/mute, brightness, status, the LFD subset; display-ID addressing over one socket per wall, on TCP 1515 or RS-232                                |
| §7 HDMI-CEC                         | `tv-rs-drivers` (`cec`)       | power on, standby, active source, over `cec-client`                                                                                                                                            |
| §8 DLNA renderer control            | `tv-rs-dlna`                  | implemented: SSDP discovery, AVTransport SOAP (`SetAVTransportURI`/`Play`/`Pause`/`Stop`/`Seek`), GENA; advertisement for a caller-hosted MediaServer; NOT a media library or ContentDirectory |
| §9 `206` + HLS serving              | `tv-rs-media`                 | range + HLS implemented; HLS segmentation (cutting segments) not started                                                                                                                       |
| §10 deferred surfaces               | —                             | not implemented: pinning verification on hardware, Samsung enterprise token math, AES-GCM framing, AirPlay — the list is [02-playbook §10](02-playbook.md#10-not-implemented-here)             |
| LG commercial serial                | `tv-rs-lgcmd`                 | `ka`/`mc` over TCP 9761 or RS-232                                                                                                                                                              |
| Host display probe (unnumbered)     | `tv-rs-local`                 | EDID + connector/mode probe; Windows WMI + registry EDID, Linux DRM, macOS declared unsupported                                                                                                |

## Workspace layout

How the tree is arranged, and why — the decision that produced the current shape.

**Flat `crates/` tree.** Every package lives at `crates/<dir>`; directories are
unprefixed, packages keep the `tv-rs-` prefix. The facade is `crates/tv-rs` → package
`tv-rs`. An earlier tree had three code homes (`src/` for the facade, `core/` for
discovery + power + transport, `crates/` for the vendor crates and the media server) and
two misleading folder names: `core/` grouped protocol crates (SSDP, Wake-on-LAN) with a
support crate (transport), and `crates/` mixed vendor crates with the media server, so
"in `crates/`" said nothing about what a crate was.

**One crate per concern, one crate per vendor protocol.** The 0.8.0 consolidation
moved the ten vendor protocols into `tv-rs-drivers` behind feature flags. The old
names remain as shims so in-repo code keeps compiling; they are never published, and
they are deleted when the last reference migrates.

**Virtual workspace root.** The root `Cargo.toml` has no `[package]`; members are a
`crates/*` glob plus `xtask`. A glob that stops matching (an emptied `crates/`) fails
the whole build with `failed to read crates\*`, which is a confusing way to learn
that a directory moved. (`crates/lgcmd` and `crates/cast` are also listed explicitly,
which the glob already covers.)

**Layering is documented, not imposed by folders.** The facade/protocol/support model and
the "protocol modules depend only on transport" rule live in this page; the dependency
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
the shell release script (a script cannot know whether a package still exists).

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
tv-rs-core → tv-rs-transport → tv-rs-drivers → tv-rs-local
  → tv-rs-dlna → tv-rs-lgcmd → tv-rs-sim → tv-rs-app → tv-rs-mirror
  → tv-rs-spatial-audio → tv-rs-vision → tv-rs-rules → tv-rs-rag
  → tv-rs-provision → tv-rs-companion → tv-rs → tv-rs-media
  → tv-rs-agent → tv-rs-bluetooth
```

`tv-rs-core` leads because every backend and the facade depend on its vocabulary;
`tv-rs-transport` follows for the crates that open a TLS channel. `tv-rs-local`,
`tv-rs-dlna`, `tv-rs-media` and `tv-rs-agent` are independent leaves, and the facade
(`tv-rs`) comes after the protocol crates it names. `tv-rs-app` is likewise a leaf.
The ten `tv-rs-<vendor>` crates (`tizen`, `webos`, `roku`, `cast`, `cec`, `discovery`,
`mdc`, `sicp`, `ir`, `power`) are deprecated, **unpublished** shims: their code lives in
`tv-rs-drivers`, so nothing published may name them as a dependency. The full rules and
the pre-release checklist are
[05-rust-package-standard §Publishing](05-rust-package-standard.md).
