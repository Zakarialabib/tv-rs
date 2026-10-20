# Glossary — and the map of these docs

Every term here is one you will meet in the code or in the protocol docs, and
each row tells you **where** to read more. Use it as the index; the numbered
pages are the long form.

## Start here

| If you are… | Read |
|---|---|
| new to the project | [README](../README.md) → this page → [01-playbook](01-playbook.md) |
| integrating it into an app | [README §Install](../README.md#install) → [glossary: facade](#facade) |
| adding a vendor | [03-matrix](03-matrix.md) → [architecture](architecture.md) |
| reviewing a release | [ADR-001](adr/001-crate-publication.md) |
| wondering why a call returns `Unsupported` | [glossary: deferred capability](#deferred-capability) |

## Architecture

**Facade** — the `tv-rs` crate: `discover()`, `identify_vendor()`, `TvClient`.
It holds no protocol logic; it classifies a device and forwards. Where: [README §How it fits together](../README.md#how-it-fits-together), [`src/client.rs`](../crates/tv-rs/src/client.rs).

**Protocol crate** — one crate per vendor or concern (`tv-rs-tizen`,
`tv-rs-webos`, `tv-rs-roku`, `tv-rs-discovery`, `tv-rs-power`). Each has the same
module shape. Where: [architecture](architecture.md).

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
credentials. Where: [01-playbook §discovery](01-playbook.md#discovery).

**UPnP descriptor** — the XML document behind `LOCATION`: friendly name,
manufacturer, model, `UDN`. Where: [`crates/discovery/src/client.rs`](../crates/discovery/src/client.rs).

**WoL (Wake-on-LAN)** — a 102-byte "magic packet" (six `0xFF` then the MAC
sixteen times) to UDP 9. Only reliable way to wake a TV from a hard-off state.
Where: [01-playbook §power](01-playbook.md#power).

**Power state** — `On` / `Standby` / `Off` / `Unknown`, from a short TCP connect:
refused means the NIC is up but services are not (`Standby`), timeout means
`Off`. Where: [`crates/power/src/types.rs`](../crates/power/src/types.rs).

**Tizen** — Samsung's TV OS. Remote channel is a WebSocket: `ws://:8001`
(legacy) or `wss://:8002` (2018+). The handshake is
`{"method":"ms.channel.connect"}`; a 2018+ set returns a `token` you present next
time instead of asking the viewer to approve. Where: [01-playbook §tizen](01-playbook.md#samsung-tizen).

**DIAL** — a REST launcher on port 8080 that starts an app by id without a
prompt. Where: [`crates/tizen/src/client.rs`](../crates/tizen/src/client.rs).

**SSAP** — LG webOS's JSON-RPC-ish WebSocket protocol on 3000 (plain) / 3001
(TLS). The first connection shows an "Allow?" prompt and returns a `client-key`.
Where: [01-playbook §webos](01-playbook.md#lg-webos).

**ECP** — Roku's External Control Protocol: an unauthenticated HTTP server on
port 8060 (`/query/device-info`, `/keypress/{key}`, `/launch/{id}`). Where:
[01-playbook §roku](01-playbook.md#roku).

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
Where: [`crates/transport/src/unsupported.rs`](../crates/transport/src/unsupported.rs).

**`TizenConnectionPool`** — one WebSocket channel per TV; a failed send reopens
it once and replays the frame, because TVs drop idle sockets. Where: [`crates/tizen/src/pool.rs`](../crates/tizen/src/pool.rs).

**`SamsungTokenStore`** — file-backed pairing tokens keyed by IP. A corrupt file
is quarantined instead of aborting startup, and an empty token never overwrites a
stored one. Where: [`crates/tizen/src/token_store.rs`](../crates/tizen/src/token_store.rs).

**`identify_vendor`** — the heuristic that turns an SSDP reply and its descriptor
into `Samsung` / `Lg` / `Roku` / `Unknown`. Deliberately narrow: unknown is a
normal answer. Where: [`src/client.rs`](../crates/tv-rs/src/client.rs).

**HIL** — hardware in the loop: tests that talk to real televisions, compiled out
unless `--features hil` is on. Where: [`tests/hardware_in_the_loop.rs`](../crates/tv-rs/tests/hardware_in_the_loop.rs).

## Process

**ADR** — architecture decision record. Where: [ADR-001](adr/001-crate-publication.md).

**Release order** — the fixed publish sequence that keeps the facade's version
dependencies resolvable on crates.io. Where: [`cargo xtask publish`](adr/001-crate-publication.md), [ADR-001](adr/001-crate-publication.md).

**Gates** — fmt, clippy, tests, rustdoc. Where: [CONTRIBUTING](../CONTRIBUTING.md).

## Device model (the v2 spec's vocabulary)

**`Capability` / `CapabilitySet`** — what a *device* can do, independent of vendor:
`PowerState`, `Wake`, `Key`, `AppLaunch`, `Foreground`, `Volume`, `InputSelect`.
A `TvClient` advertises exactly the set its vendor supports, and the trait methods
enforce it — asking outside the set is `TvError::Unsupported`, not a silent no-op.
Where: `crates/core/src/capability.rs`.

**`DeviceHandle`** — the erased handle a fleet holds: `Arc<dyn DeviceHandle>` plus the
five capability traits (`PowerControl`, `KeyInput`, `AppLaunch`, `InputSelect`,
`VolumeControl`). No generics, no vendor enum in the type. Where:
`crates/core/src/capability.rs`, implemented for `TvClient` in `crates/tv-rs/src/handle.rs`.

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

**`power_observation()`** — the facade call that returns the power state *and* its
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
`Retry`, `BackOff`, `Reprovision`, `Wait`, `Disable`, `ApplyQuirk`. A missing or
expired credential is `Reprovision`, never `Retry`: retrying a dead token is how a
fleet locks a device out.

**Conformance suite** — the shared test every `DeviceHandle` must pass (capability and
accessor agreement, unsupported routing, identity stability), behind the
`conformance` feature. Where: `crates/core/src/conformance.rs`.
