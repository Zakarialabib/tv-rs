# Documentation

Fifteen published crates, one facade, one idea: speak each TV's own protocol and never
hide a failure behind a panic. This page is both the **index** — it routes to
everything in `docs/` and to the root pages beside it — and the **term map**: every
word you will meet in the code or the protocol docs, with the place that explains it.

## Read in this order

| #   | Page                                                                                                                   | What it answers                                                                                                                                                                    |
| --- | ---------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 0   | **`00-index.md`** (this page)                                                                                          | The router and the term map. Start here if you only have five minutes.                                                                                                             |
| 1   | [`01-architecture.md`](01-architecture.md)                                                                             | The workspace shape, the layers, the dependency rule, the module inventory per crate, how the repo is laid out, and the crates.io publish order.                                   |
| 2   | [`02-playbook.md`](02-playbook.md)                                                                                     | The protocol facts, numbered §1–§10: ports, handshakes, MDC frames, DLNA, media, HDMI-CEC, what each vendor exposes, and what is deliberately not implemented.                     |
| 3   | [`03-matrix.md`](03-matrix.md)                                                                                         | State: what each crate implements today, the **capability truth** table, the registry versions, the verified test counts, platform truth, the peer landscape, and the honest gaps. |
| 4   | [`04-external-refs.md`](04-external-refs.md)                                                                           | Protocol references, prior art, and the crates this workspace learned from.                                                                                                        |
| 5   | [`05-rust-package-standard.md`](05-rust-package-standard.md)                                                           | The engineering standard the whole workspace follows — reusable outside this repo.                                                                                                 |
| 6   | [`06-plan.md`](06-plan.md)                                                                                             | **The one plan.** Open work only, with an exit criterion per item, plus the expansion proposal as an appendix. Landed work lives in the CHANGELOG.                                 |
| 7   | [`07-adoption.md`](07-adoption.md)                                                                                     | The contract for adopting this workspace inside an application: what the app owns, what the crate owns, the no-duplication rules.                                                  |
| 8   | [`08-hil-results.md`](08-hil-results.md)                                                                               | What has actually run against a physical TV. Today: nothing.                                                                                                                       |
| —   | [`../INTEGRATION.md`](../INTEGRATION.md)                                                                               | **Putting it in an application**: discovery, TLS pinning, quirks, a fleet, media, CEC, the pre-ship checklist.                                                                     |
| —   | [`../README.md`](../README.md)                                                                                         | The face of the project: what it is, install, the ten-line example, the crate list.                                                                                                |
| —   | [`../QUIRKS.md`](../QUIRKS.md)                                                                                         | Submitting a model/firmware quirk row.                                                                                                                                             |
| —   | [`../NON_GOALS.md`](../NON_GOALS.md) · [`../SECURITY.md`](../SECURITY.md) · [`../CONTRIBUTING.md`](../CONTRIBUTING.md) | What is deliberately out of scope, how to report a vulnerability, and the gates a change must pass.                                                                                |
| —   | [`../CHANGELOG.md`](../CHANGELOG.md)                                                                                   | What has already landed, release by release. This is the history; `06-plan.md` is the future.                                                                                      |

## Conventions used here

- **Claims about code are checked against code.** If a page says a function
  exists, it exists in `crates/`. When the two disagree, the code is right and the
  page is a bug.
- **Deferred is a feature.** A capability that is not implemented returns a typed
  `Unsupported` error with a reason. No page promises something the code refuses
  to do.
- **Numbers are dated.** Test counts and the registry table in `03-matrix.md`
  carry the date they were measured; they rot, the source does not.
- **The plan is one page.** `06-plan.md` is the only plan; an item leaves the page
  when it lands. What landed is the [`CHANGELOG.md`](../CHANGELOG.md); a page
  that accumulates ticks is a second changelog.
- **The numbering is a role, not a sequence.** `01-` is the system description,
  `02-` the protocol facts, `03-` the state, `06-` the plan. The expansion
  proposal lives as an appendix of the plan rather than as its own page.

## Where the code lives

| Path                                                               | Contents                                                                                                                                          |
| ------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| `crates/core`                                                      | the device model: capability traits, the erased `DeviceHandle`, `CommandId`, observed state, the error taxonomy                                   |
| `crates/tv-rs`                                                     | facade: discovery orchestration, vendor identification, `TvClient`                                                                                |
| `crates/{discovery,power,tizen,webos,roku,mdc,cec,bluetooth,cast}` | one wire protocol each (SSDP, Wake-on-LAN, Tizen, webOS, Roku ECP, Samsung MDC, HDMI-CEC) plus Bluetooth HID emulation and the Google Cast sender |
| `crates/local`                                                     | the host's own display (EDID/mode probing)                                                                                                        |
| `crates/{transport,dlna,media}`                                    | support: TLS/frame reader; DLNA renderer transport; `206`/HLS server                                                                              |
| `crates/app`                                                       | the TV app producer: a supplied payload in, `.wgt` / webOS app directory / web bundle out                                                         |
| `crates/agent`                                                     | reconciliation: desired state in, fleet converged, audited                                                                                        |
| `crates/cli`                                                       | the reference consumer (`publish = false`)                                                                                                        |
| `xtask`                                                            | workspace automation: `cargo xtask publish`, `cargo xtask hw-test`                                                                                |
| `quirks/`                                                          | the quirks table as data                                                                                                                          |
| `docs/`                                                            | these pages                                                                                                                                       |

---

# Term map

Every term below is one you will meet in the code or the protocol docs, and each row
tells you **where** to read more. The numbered pages are the long form.

## Start here

| If you are…                                | Read                                                                                    |
| ------------------------------------------ | --------------------------------------------------------------------------------------- |
| new to the project                         | [README](../README.md) → this page → [02-playbook](02-playbook.md)                      |
| integrating it into an app                 | [README §Install](../README.md#install) → [07-adoption](07-adoption.md)                 |
| adding a vendor                            | [03-matrix](03-matrix.md) → [01-architecture](01-architecture.md)                       |
| cutting a release                          | [06-plan §Release](06-plan.md) → [05-standard §Publishing](05-rust-package-standard.md) |
| wondering why a call returns `Unsupported` | [02-playbook §10](02-playbook.md#10-not-implemented-here)                               |

## Architecture

**Facade** — the `tv-rs` crate: `discover()`, `identify_vendor()`, `TvClient`.
It holds no protocol logic; it classifies a device and forwards. Where: [README §How it fits together](../README.md), [`src/client.rs`](../crates/tv-rs/src/client.rs).

**Protocol crate** — one crate per vendor or concern (`tv-rs-tizen`,
`tv-rs-webos`, `tv-rs-roku`, `tv-rs-discovery`, `tv-rs-power`). Each has the same
module shape. Where: [01-architecture](01-architecture.md).

**Transport crate** — `tv-rs-transport`: the TLS policy and connector, and the
budgeted WebSocket frame reader both vendor crates share. Where: [`crates/transport/src/`](../crates/transport/src/).

**Aggregate client** — `TvClient`, the enum that wraps a vendor client and
exposes only the operations every vendor supports (`power_off`, `launch_app`)
plus typed accessors (`as_tizen`, `as_webos`, `as_roku`). Where: [`src/client.rs`](../crates/tv-rs/src/client.rs).

**Media server** — `tv-rs-media`: an HTTP server that answers `Range` requests
with `206 Partial Content` and renders HLS playlists. Deliberately separate so
`axum` never enters a protocol crate's dependency graph. Where: [`crates/media/src/`](../crates/media/src/).

## Protocols

**SSDP** — multicast discovery over UDP 1900. An `M-SEARCH` datagram goes out,
devices answer with a `LOCATION` header pointing at an XML description. No
credentials. Where: [02-playbook §1](02-playbook.md#1-discovery).

**UPnP descriptor** — the XML document behind `LOCATION`: friendly name,
manufacturer, model, `UDN`. Where: [`crates/discovery/src/client.rs`](../crates/discovery/src/client.rs).

**WoL (Wake-on-LAN)** — a 102-byte "magic packet" (six `0xFF` then the MAC
sixteen times) to UDP 9. Only reliable way to wake a TV from a hard-off state.
Where: [02-playbook §2](02-playbook.md#2-power).

**Power state** — `On` / `Standby` / `Off` / `Unknown`, from a short TCP connect:
refused means the NIC is up but services are not (`Standby`), timeout means
`Off`. Where: [`crates/power/src/types.rs`](../crates/power/src/types.rs).

**Tizen** — Samsung's TV OS. Remote channel is a WebSocket: `ws://:8001`
(legacy) or `wss://:8002` (2018+). The handshake is
`{"method":"ms.channel.connect"}`; a 2018+ set returns a `token` you present next
time instead of asking the viewer to approve. Where: [02-playbook §3](02-playbook.md#3-samsung-tizen).

**DIAL** — a REST launcher on port 8080 that starts an app by id without a
prompt. Where: [`crates/tizen/src/client.rs`](../crates/tizen/src/client.rs).

**SSAP** — LG webOS's JSON-RPC-ish WebSocket protocol on 3000 (plain) / 3001
(TLS). The first connection shows an "Allow?" prompt and returns a `client-key`.
Where: [02-playbook §4](02-playbook.md#4-lg-webos).

**ECP** — Roku's External Control Protocol: an unauthenticated HTTP server on
port 8060 (`/query/device-info`, `/keypress/{key}`, `/launch/{id}`). Where:
[02-playbook §5](02-playbook.md#5-roku).

**MDC** — Samsung Multiple Display Control, the protocol commercial signage
panels expose where a consumer TV has nothing to automate: framed commands with
a trailing checksum, addressed by display id, over TCP 1515 by default or an
RS-232 line behind the `serial` feature. The panel-lock, orientation and
health capabilities come from here, not from the consumer remote channels.
Where: [02-playbook §6](02-playbook.md#6-samsung-mdc-commercial-panels),
[`crates/mdc/README.md`](../crates/mdc/README.md).

**CEC** — the HDMI bus's own remote-control channel: power on, standby and active
source over the HDMI cable, driven here through a `cec-client` subprocess. Where:
[02-playbook §7](02-playbook.md#7-hdmi-cec-panels-the-network-cannot-reach).

**DLNA / AVTransport / GENA** — UPnP MediaRenderer control: SSDP discovery,
AVTransport SOAP actions (`SetAVTransportURI`, `Play`, `Pause`, `Stop`,
`Seek`), GENA eventing (`SUBSCRIBE`/renew/`UNSUBSCRIBE`). Transport only —
never a media library or ContentDirectory. Where:
[02-playbook §8](02-playbook.md#8-dlna-renderer-control),
[`crates/dlna/src/lib.rs`](../crates/dlna/src/lib.rs).

**LFD subset** — the limited-feature-display capabilities a commercial panel
has and a consumer TV channel does not: `PanelLock`, `Orientation`,
`NoSignalStatus` (and behind the core's `supporting` feature,
`LampOrPanelHours` and `ScreenCapture`). The honest per-backend table lives in
[03-matrix §Capability truth](03-matrix.md#capability-truth).

**HLS** — an `.m3u8` playlist of media segments; the portable format for a TV
player. Where: [`crates/media/src/hls.rs`](../crates/media/src/hls.rs).

**HTTP 206** — partial content. A player that cannot get a `206` with the right
`Content-Range` will not let the viewer seek. Where: [`crates/media/src/range.rs`](../crates/media/src/range.rs).

## Code vocabulary

**`TlsPolicy`** — `Strict` (validate the certificate), `Pinned` (verify one fingerprint),
`TrustOnFirstUse` (record on the first handshake, verify afterwards) or `AcceptInvalid` (accept
a self-signed TV certificate). One definition, in `tv-rs-transport`, because its
consequences are a workspace-wide security property. Where: [`crates/transport/src/tls.rs`](../crates/transport/src/tls.rs).

**Deferred capability** — a call that needs authentication behaviour this project
does not implement (certificate pinning, token derivation, encrypted framing). It
returns an `Unsupported` error carrying a phase and a reason, and never panics.
Where: [`crates/transport/src/unsupported.rs`](../crates/transport/src/unsupported.rs);
the full list is [02-playbook §10](02-playbook.md#10-not-implemented-here).

**`TizenConnectionPool`** — one WebSocket channel per TV; a failed send reopens
it once and replays the frame, because TVs drop idle sockets. Where: [`crates/tizen/src/pool.rs`](../crates/tizen/src/pool.rs).
The facade's Tizen variant holds one, so a key press reuses the channel instead of
paying a handshake per press.

**`WebosSessionPool`** — the same idea for LG: one registered SSAP session per TV,
because connecting without a stored `client-key` raises the on-screen "Allow?"
prompt. Where: [`crates/webos/src/pool.rs`](../crates/webos/src/pool.rs).

**`DynConnectionPool` / `DynSessionPool`** — a pool with its factory erased
(`TizenConnectionPool<Box<dyn TizenChannelFactory + Send + Sync>>` and its webOS
twin), which is what lets a facade store one pool type while a caller still supplies
its own factory. Where: [`crates/tizen/src/pool.rs`](../crates/tizen/src/pool.rs),
[`crates/webos/src/pool.rs`](../crates/webos/src/pool.rs).

**`TvClient::close_all()`** — drops every pooled connection the handle owns and
returns how many; the lever a mobile app pulls when it goes to the background,
since the OS will not close a WebSocket for it. Where: [`crates/tv-rs/src/handle.rs`](../crates/tv-rs/src/handle.rs).

**`testkit`** — the counting channel/session factories behind the `testkit` feature of
`tv-rs-tizen`, `tv-rs-webos` and `tv-rs` (off by default). They are how connection
reuse is proved in CI without a television: twenty key presses through the facade must
open exactly one channel. Where: [`crates/tizen/src/pool.rs`](../crates/tizen/src/pool.rs).

**`SamsungTokenStore`** — file-backed pairing tokens keyed by IP. A corrupt file
is quarantined instead of aborting startup, and an empty token never overwrites a
stored one. Where: [`crates/tizen/src/token_store.rs`](../crates/tizen/src/token_store.rs).

**`identify_vendor`** — the heuristic that turns an SSDP reply and its descriptor
into `Samsung` / `Lg` / `Roku` / `Philips` / `Unknown`. Deliberately narrow: unknown is a
normal answer. Where: [`src/client.rs`](../crates/tv-rs/src/client.rs).

**HIL** — hardware in the loop: tests that talk to real televisions, compiled out
unless `--features hil` is on. Where: [`tests/hardware_in_the_loop.rs`](../crates/tv-rs/tests/hardware_in_the_loop.rs),
results in [08-hil-results](08-hil-results.md).

**`discover_tvs_with(callback)`** — discovery that hands each device over as its
descriptor resolves, so a television that answers in 40 ms surfaces then rather than
after the slowest reply in the window. The socket loop is written out rather than shared
with `msearch` because the descriptor fetch has to `await` inside it. Where: [`crates/discovery/src/client.rs`](../crates/discovery/src/client.rs).

**`mdc_handle(address)`** — the facade's entry point for a commercial Samsung panel,
returning the same erased `DeviceHandle` the rest of a fleet is driven through.
Deliberately **not** reached through `TvClient` and **not** classified from discovery:
SSDP carries no authoritative vendor field, so a panel answers it exactly as a consumer
set does, and `DeviceOs::SamsungMdc` is never produced by a reply. The caller knows — an
operator adding a panel by address, or a probe that found MDC listening. Where:
[`crates/tv-rs/src/client.rs`](../crates/tv-rs/src/client.rs).

**`platform-verifier`** — the `tv-rs-transport` feature that asks the operating system for
a root store (`rustls-platform-verifier`) instead of reading one with
`rustls-native-certs`. It exists because Android exposes a keystore and iOS its own roots,
neither of which that call can see, so a mobile build with `TlsPolicy::Strict` had no roots
to verify against. Additive: `Pinned` and `TrustOnFirstUse` install their own verifiers and
never reach it, so it cannot weaken pinning. Where: [`crates/transport/src/tls.rs`](../crates/transport/src/tls.rs).

**`installed_apps()` / `type_text()`** — the two facade reads and writes added alongside
their capabilities. The first enumerates (webOS `listApps`, Roku `/query/apps`, Tizen
refusing); the second types a string (Roku `keypress/Lit_<char>`, printable ASCII only,
refused atomically so a refusal never leaves half a search term typed). Where:
[`crates/tv-rs/src/handle.rs`](../crates/tv-rs/src/handle.rs).

## Process

**Gates** — fmt, clippy, tests, rustdoc. Where: [CONTRIBUTING](../CONTRIBUTING.md),
[05-rust-package-standard §CI Gates](05-rust-package-standard.md).

**Release order** — the fixed publish sequence that keeps the facade's version
dependencies resolvable on crates.io: `cargo xtask publish`, one crate at a time, in
dependency order. Where: [01-architecture §Publishing](01-architecture.md#publishing).

## Device model vocabulary

**`Capability` / `CapabilitySet`** — what a _device_ can do, independent of vendor.
Seventeen variants: `PowerState`, `Wake`, `Key`, `AppLaunch`, `Foreground`,
`InputSelect`, `Volume`, `Brightness`, `Reboot`, `Telemetry`, `PanelLock`,
`Orientation`, `NoSignalStatus`, `AppList` and `TextInput` as always-on surfaces, plus
`LampOrPanelHours` and `ScreenCapture` behind the core's `supporting` feature. The last
two declared are the narrowest on purpose: listing what is installed is not launching an
application, and taking a string is not pressing a key — a platform can do one and not
the other, and the refusal names the capability it actually lacks. A backend advertises
exactly the set it supports, and the trait accessors enforce it — asking outside
the set is `TvError::Unsupported`, not a silent no-op. Where:
`crates/core/src/capability.rs`; the honest per-backend table:
[03-matrix §Capability truth](03-matrix.md#capability-truth).

**`DeviceHandle`** — the erased handle a fleet holds: `Arc<dyn DeviceHandle>` plus
the capability traits (`PowerControl`, `KeyInput`, `TextInput`, `AppLaunch`,
`InputSelect`, `VolumeControl`) and the LFD surfaces (`PanelLockControl`, `OrientationControl`,
`PanelHealthProvider`; behind `supporting`: `PanelHoursProvider`,
`ScreenCapturer`). No generics, no vendor enum in the type. Where:
`crates/core/src/capability.rs`; implemented for `TvClient` in
`crates/tv-rs/src/handle.rs` and for the MDC device in `crates/mdc/src/handle.rs`.

**`CommandId`** — the caller's correlation id for a command. The contract is explicit:
backends do **not** deduplicate, so a retry is a second press unless the caller checks.
Where: `crates/core/src/command.rs`.

**`Confidence`** — how a value was learned: `High` (the device reported it), `Inferred`
(a write was accepted, or a probe answered), `Unknown` (nothing, or the value aged
out). `ObservedField<T>` carries it with `value` and `last_verified_at`.

**`ObservedField` / `ObservedState`** — a confidence-tagged reading. The constructors
are the discipline: `from_read` is the only one that yields `High`;
`from_unverified_write` and `from_toggle` yield `Inferred`; `from_inference` is for
side channels such as the TCP power probe. Where: `crates/core/src/state.rs`.

**`power_observation()`** — the facade call that returns the power state _and_ its
confidence. `power_state()` is a projection of it, so the two cannot disagree.

**`Quirk` / `QuirkAction`** — a model/firmware defect and what to do about it, as data
(`quirks/default.toml`). `QuirkAction` decodes the row's `action` into something code
can obey — `StartAtPort`, `ExpectAuthRequired`, `TrustDeviceInfoFlags`, `PromptUser` —
and a test refuses any shipped action the crate cannot act on.

**Pinned port** — `TizenClient::pinned_port` / `WebosClient::pinned_port`: use exactly
one port, no ladder. The facade sets it from a quirk row, so a firmware that dropped
8001 never costs a refused connection.

**`decide()` / `Reconciler`** — the agent's pure core and its loop.
`decide(desired, observed, capabilities) -> Decision` touches no I/O;
`Reconciler::tick()` runs one pass over the fleet and `run(stop)` repeats it.
Where: `crates/agent/src/`.

**`Provider`** — where desired state comes from: `StaticFileProvider`,
`HttpPollProvider` (feature `http`), `MqttSubscribeProvider` (feature `mqtt`). The
library owns the loop; the product owns the source.

**`HealerLadder` / `Remedy`** — the escalation from a failure to an action:
`Retry`, `BackOff`, `Reprovision`, `Wait`, `Disable`, `Quarantine`, `ApplyQuirk`.
A missing or expired credential is `Reprovision`, never `Retry`: retrying a dead
token is how a fleet locks a device out. `Quarantine` means the capability still
exists but the device's condition forbids using it.

**Graduated remediation ladder** — the telemetry-driven escalation in
`tv-rs-agent::remedy`: a hot-but-not-critical panel is **dimmed**, a panel whose
temperature or link is critical is **rebooted once** and quarantined either way,
a live input whose player stopped is **restarted**. Every rung is a pure
function of (snapshot, verdict, capabilities); a rung whose capability is
missing is skipped, never faked. Where: `crates/agent/src/remedy.rs`.

**Conformance suite** — the shared test every `DeviceHandle` must pass (capability and
accessor agreement, unsupported routing, identity stability), behind the
`conformance` feature. Where: `crates/core/src/conformance.rs`.

**App status** — what a Samsung Tizen set reports about one installed application:
`GET /api/v2/applications/{app_id}` (no pairing, no token), answering `id`, `name`,
`running` and `visible`. `running` is a **superset** of foreground — background system
apps report `running: true` while not on screen — so `visible` is the marker a caller
must read, and a panel that omits `visible` answers `None`, which is not `false`.
Where: `crates/tizen/src/payload.rs` (`TizenAppStatus`, `parse_app_status`).

**RenderingControl** — the UPnP service that carries volume and mute, separate from
AVTransport (casting/playback) and with its own control URL, so a renderer can be
castable without exposing volume. Exposed as free functions (`get_volume`, `set_volume`,
`get_mute`, `set_mute`) and behind the `RenderingControl` trait
(`SoapRenderingControl`); `Renderer.rendering_control_url` is `None` for a renderer that
has none. Where: `crates/dlna/src/soap.rs`.

**ServiceNotFound** — the DLNA error for a device that exposes no service of the type
asked for. It carries the service URN, so "no volume control" and "cannot be cast to"
stay distinguishable rather than collapsing into one failure. Replaced
`AvTransportNotFound` (which could only name one service). Where:
`crates/dlna/src/error.rs`.
