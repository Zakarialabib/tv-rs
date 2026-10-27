# Plan

**This is the only plan.** It holds open work and nothing else: an item exists here
only while it is **not** done, and when it lands its section is deleted. Landed work is
[`CHANGELOG.md`](../CHANGELOG.md); what each crate implements today is
[`03-matrix.md`](03-matrix.md); why the architecture is what it is is
[`01-architecture.md`](01-architecture.md). This page never grows a history.

Two things are **proposals, not commitments**, and are marked as such where they
appear: [Appendix A](#appendix-a--expansion-proposal-industry-ports-and-new-layers)
(vendor ports and new layers) and [item 4](#4-tv-rs-sony--dropped) (a dropped
candidate recorded so it is not re-added). Only items 1–9 below are queued.

---

## 1. `tv-rs-app` — the TV app producer (SignageMaster handoff E6)

Package a **supplied** web/wasm payload as an installable TV app: a Tizen `.wgt`, a
webOS app directory, and a plain web bundle. This is the one handoff item the
SignageMaster refactor spec
(`Simple-Signage/docs/specs/ARCHITECTURE-REFACTOR-SPEC.md`, card E6) hands to this
workspace, and it is deliberately narrow:

- **Generic payload, no SignageMaster types.** The crate takes a payload directory
  plus a descriptor (id, name, version, entry point). SignageMaster's no_std reducer
  is one payload among others; player logic, scheduling and content stay out, per
  [`NON_GOALS.md`](../NON_GOALS.md).
- **What is TV knowledge, and therefore lives here**: app-manifest generation (Tizen
  `config.xml`, webOS `appinfo.json`), the bundle layout, and D-pad key mapping for
  both remotes.
- **Unsigned artifacts, no device install.** Signing and installing stay out of scope
  until [`08-hil-results.md`](08-hil-results.md) has a row for them.
- **`.wgt` is a ZIP**, so it is written here. A `.ipk` is an `ar` archive of control
  and data tarballs: assembling one needs the webOS CLI (`ares-package`), so the
  crate emits the webOS app directory and returns a typed error naming that tool
  rather than pretending to have built an installable package.

**Where this stands — and it is done on this side.** The crate exists, is tested
(descriptor validation, both manifests, the bundle, the ZIP writer, the generated bridge
and the typed `.ipk` refusal: 42 tests), the `.wgt` it produces was opened by two
independent readers (Python's `zipfile`, which also verified every CRC, and Windows'
`Expand-Archive`), and **it rides the `0.3.0` release**
([registry table](03-matrix.md#published-cratesio-verified-2026-10-22-against-the-registry-api)).
The only thing left is the consumer deleting its own wrapper — which is SignageMaster's
work, not this workspace's, and is why this section is only waiting on a downstream
commit rather than on code.

**Exit:** `tv-rs-app` publishes; Tizen and web bundles are produced from a payload and
open with the platform's own tooling; the manifest and key-map functions are covered
by tests; SignageMaster deletes `platform-tv-wrapper` once this publishes.

## 1b. Display probe merge (SignageMaster handoff E3b) — the consumer switch

SignageMaster's `crates/device/src/hardware/display/` duplicated what
`tv-rs-local` is for. **The tv-rs side landed on 2026-10-23**
([CHANGELOG](../CHANGELOG.md)): a public, platform-independent EDID parser
(`tv_rs_local::parse_edid` → `Edid`, with the preferred `DetailedTiming`), and
`display_edids()`, which reads each **active** monitor's registry EDID on Windows
(keyed by the device interface path, so stale registry entries never leak in) and
`/sys/class/drm/*/edid` on Linux. The Windows probe now falls back to that EDID when
WMI has no record. Tested against two real EDIDs (an LG TV and a Samsung C49RG9x,
linuxhw/EDID, CC BY 4.0) and run on a live Windows host.

What is open is the consumer's half: SignageMaster replaces `edid::parse_edid_bytes`
(and its `edidkit` dependency) and `read_edid_from_registry` with the calls above,
then deletes its copy. Not moved, by design: DDC/CI brightness and display rotation
in its `win32.rs` are output *control*, which is [item 6](#6-ddcci-in-tv-rs-local)
and Appendix A's `tv-rs-local` v2 — not part of the probe.

**Exit:** SignageMaster depends on `tv-rs-local` for EDID identity and deletes
`edid.rs`; this section is deleted. **The tv-rs half is complete** — what this section now
waits on is the consumer's commit, not any work in this repository.

## 2. Companion-app gaps — the tv-rs side

Requested by the companion app (`companion-app/docs/TV-RS-GAPS.md`, ranked by its
`BACKEND-PLAN.md`). Every gap below was verified open on `main` before it was written
down. **One PR per gap, with its test and a CHANGELOG entry; the section is deleted
when it lands.** The gap work ships in [0.4.0](#9-release-040).

Tier A shapes the contract, so it goes before the companion writes its `src-tauri`.
Tier B makes the Remote real, per vendor — reorder it by the TVs on the bench.

Landed so far, each recorded in [CHANGELOG.md](../CHANGELOG.md): **A2** (one error
type, exhaustive remedy maps, the webOS registration split), **A1** (the facade owns
the connection pools, with `close_all()` and a `testkit` proof that twenty presses
open one channel), **A3** (the core `serde` feature and its wire contract), **A4**
(wake on all three vendors, `with_mac`, `learn_mac` on Roku, and `Capability::Wake`
advertised only where it can be delivered), **A5** (a platform-refused call is
`Blocked`/`RequestPermission`, not an unreachable television) and **B2's Roku half**
(`/query/active-app`).

### 2.1 A4 · G4 (the remainder) — MAC learning on Samsung and webOS

Wake landed for all three vendors, and G13 added the fallback: Roku goes over ECP
`PowerOn` first, then falls back to a magic packet when the set is fully off and a caller
has supplied an address; Tizen and webOS broadcast the packet directly, because they drop
their network stack when they sleep. `TvClient::with_mac` supplies that address, and
`Capability::Wake` is advertised only where it can actually be delivered. What is open is
the convenience half — learning the address instead of asking for it.

`TvClient::learn_mac()` answers on Roku (`/query/device-info`, Wi-Fi address preferred
over wired) and now on **Samsung** — `TizenClient::device_info()` reads
`GET http://<ip>:8001/api/v2/`, which needs no pairing, so the address is learnable before
the remote channel is ever opened. `DeviceInfo` carries `network_type` alongside the
address because the reported MAC is the Wi-Fi one even on a wired set; a test pins that a
MAC-shaped `ssid` is not read as an address, and another that a document without a
`device` object is a protocol error rather than "no MAC".

**webOS keeps refusing, and the reason is now precise rather than "untraced."** The two
endpoints the item named return no MAC at all: `getSystemInfo`'s key set is closed
(`boardType`, `firmwareVersion`, `modelName`, `sdkVersion`, `UHD`, `_3d`), and LG staff
have said twice — 2023 and 2025 — that no MAC API is provided. The one endpoint that does
carry a MAC (`com.webos.service.connectionmanager/getinfo`) is OSE-only, absent from LG's
consumer documentation, gated at API level 29, and lgtv2 swallows its failure so it cannot
distinguish working from refused. That is a refusal with evidence behind it, not a gap in
our research. Refusing
is the honest answer until someone traces one.

**This half is closed as a refusal with evidence, not left open.** The exit below asked
for `learn_mac()` on *both* vendors; webOS cannot satisfy it, and the evidence above is why
that is a result rather than an unfinished task. What remains on A4 is nothing this
workspace can do: the address is not obtainable on a consumer LG panel through any
documented surface.

**Exit (met for Roku and Samsung; refused with evidence for webOS):** provenance-traced
sources in [`02-playbook.md`](02-playbook.md), `learn_mac()` answering on both vendors that
provide one, and a test pinning the parsed field.

### 2.2 B2 · G10b (the remainder) — foreground on Tizen

Roku landed (`/query/active-app`) and the facade's `foreground()` answers there. Tizen is
open, and the trace is now **conclusive** — the plan's own premise is dead, and a verified
path exists in its place.

**The plan's premise fails the two-source bar.** The item assumed `GET /api/v2/applications`
(the bare list) would answer with a foreground marker. **No source, capture, or client issues
that call.** Not in `ha-samsungtv-smart` (which uses a WebSocket method), not in
`samsung-tv-ws-api` (whose `rest.py` has a per-app status method and *no* list method at
all), and not in the four panel captures in `cleanyourtv` issue #441 — every one of them
uses the `/{id}` form. Implementing the bare list would build an endpoint nobody has observed,
so it stays unimplemented.

**What is verified, and is the way to close the gap:**

```
GET http://<ip>:8001/api/v2/applications/{appId}      # no pairing, no token
  -> 200 {"id":..,"name":..,"running":bool,"version":..,"visible":bool}
  foreground == entry.visible == true
```

Four independent sources agree on that body: the FHEM thread's verbatim captures (topic 57595
p.51), `ha-samsungtv-smart`'s production code (`_set_running_app()` reads
`result.get("visible")` as the foreground marker), `cleanyourtv` #441 (four panels, all
`200`, with `404` for an absent app), and `dzikus/tizen-camera-pip` (id parity with the launch
call). So the call is real — the *list* was not.

**Three traps, all verified, all worth a test each:**

1. **`running` is not foreground.** It is a superset: background system apps report
   `running:true, visible:false`. A caller keying on `running` alone reports a background
   app as the one on screen. The FHEM author's own note says exactly this ("Netflix startet
   automatisch und läuft im Hintergrund" — so he asks whether the app is *sichtbar*).
2. **`visible` can be absent** on some firmware/models (a Home-Assistant thread hit
   `KeyError: 'visible'`). A missing field is **not** `false` — it means "this panel does
   not tell me", and that must not be reported as a background app.
3. **`POST /api/v2/applications/{id}` is a launch**, not a status: it returns a bare `true`
   with `content-length: 5`. Do not read the response of the launch as an app's state.

**The first attempt at this correction was wrong, and the correction is the point.** It said
"the facade already lists installed apps, so iterate those ids". It does not: Tizen's
`installed_apps()` returns `Unsupported(Capability::AppList)` — **Tizen cannot enumerate apps**
over any verified surface. So there are no ids to iterate, and `foreground()` cannot be built
from this endpoint alone.

**What shipped instead** (`feat(tizen)`): `TizenClient::app_status(app_id)` — the verified
query, with `TizenAppStatus` carrying `running` and `visible` as **distinct** fields. Six
tests pin the traps, including the one that would have been easy to get wrong: a background
app reports `running: true`, and an absent `visible` parses as `None` rather than `false`.

**What is left, and why it is not guessed:** the id source. The REST list is unverified, and
the DIAL manifest — which `apps.rs` names as "list[ing] what is installed" — has **no parser
anywhere in the workspace**: `dial_device_description()` returns raw XML, there is **no DIAL
XML fixture in the repo at all**, and the facade reads that description with a substring
check for `"samsung"`. Parsing it would mean inventing both the endpoint and the XML shape,
which this workspace does not do. So `foreground()` on Tizen stays refusing, and the refusal
now has a precise reason: the endpoint is done and tested, the ids are not.

**The trace came back conclusive, and it splits the item in two.** Enumeration exists, but
only over the WebSocket remote channel, and only on older sets:

- **On Tizen ≤4 (roughly pre-2020) the TV answers the list.** Send
  `{"method":"ms.channel.emit","params":{"event":"ed.installedApp.get","to":"host"}}` on the
  remote-control channel and the set replies with an **unsolicited event frame** whose
  `data.data[]` is the installed-app list — `{appId, app_type, icon, is_lock, name}`. Four
  independent sources: `xchwarze/samsung-tv-ws-api` (two files), `ollo69/ha-samsungtv-smart`,
  an openHAB TRACE log carrying a verbatim TV body, and `Ape/samsungctl` issue #75. The
  `appId` values are **exactly** what the per-app status endpoint consumes, so the two halves
  of this item join.
- **On Tizen 5+ (2020 and later) Samsung removed it.** The command is *silently ignored* —
  no error frame, no reply, the client hangs until it times out. Verified by four sources
  including a maintainer's issue and a library's own documentation ("Many newer TVs no longer
  support this request, so there is no list"). There is no HTTP and no DIAL enumeration path
  on those sets, so **a caller must supply ids**.

**Three traps, all verified, each worth a test:**

1. **It is not a request/response pair.** The reply is an event frame arriving on the same
   socket; code that awaits an inline answer loses the list. This is also why it cannot ride
   the pool's existing `dispatch`, which is built for request/response.
2. **Silence is the failure mode on new sets**, so the honest answer is a typed error naming
   the removal — never an empty list, which would read as "this TV has no apps installed".
3. **`app_type` decides how the app is launched** (`2` = DEEP_LINK, `4` = NATIVE_LAUNCH), and
   the wrong action type fails with a WebSocket close `31522`. The list is therefore not just
   ids: whoever stores it must keep the type with it.

**The `apps.rs` comment is refuted.** It asserted that "the DIAL manifest lists what is
installed". It does not: the DIAL device description carries `serviceList` and nothing else —
confirmed against two captured bodies (a 2015 UN40HU7000 and a 2020 UA50RU7400RSMZ) — and no
client issues DIAL's `/apps` endpoint on a Samsung set at all. The comment has been corrected
in the source rather than only here, because it was the kind of unverified claim that reads as
settled fact.

**What this means for the item:** `foreground()` on Tizen stays refusing until the list is
implemented, and when it is, it can only answer on the sets that enumerate. That is a
capability difference between firmware generations, not a gap in this workspace — and it
belongs in the matrix's vendor table as such.

### 2.3 D — the rest, in the companion's order

Each entry was checked against the code before being left standing, because these
premises go stale: what the audit found is recorded next to the item. Landed items are
gone from the list — G3 (streaming discovery), G8 (release from quarantine) and G10e (MDC
behind the facade) all landed, and their rationale now lives in the CHANGELOG and the
term map ([`00-index.md`](00-index.md#term-map)).

**G10d landed**, and its premise was right in the way that matters: `soap.rs` did
already parse the `RenderingControl:1` service out of the device description, and the
actions were genuinely missing. `GetVolume`/`SetVolume`/`GetMute`/`SetMute` now exist as
free functions and behind a `RenderingControl` trait, with `Renderer` carrying
`rendering_control_url` alongside `av_transport_url`. Two things the implementation had
to decide, both worth recording:

- **The two services share one HTTP path.** `post_soap`/`send_soap` hardcoded the
  AVTransport URN in the `SOAPACTION` header, so RenderingControl needed the service
  type as a parameter — a private function, so no shim and no duplication of the
  plumbing. The public `extract_av_transport_control_url` became
  `extract_service_control_url(xml, base, service_urn)`, with both callers updated.
- **The range check could not be a cast.** `u8::try_from(101)` succeeds — `u8` holds
  0..=255 — so the declared 0..=100 bound has to be an explicit comparison. The test
  suite caught this one: `101` was accepted as a volume until the bound was written
  out.

**G15 was already done when the audit read it, and the audit was wrong.** `tv_rs::probe()`
exists and answers exactly the item's question — Roku first (authoritative, carries the
MAC, model and serial), then Tizen through its DIAL description, then LG as a port
inference, with the reasoning for each step written into its doc comment. The audit's
"nothing probes a single address today" was false, and it was false in the same way G16's
was: a grep that found no *new* function and concluded no *function*. Recorded because the
next reader will otherwise repeat the mistake — and because it nearly cost a duplicate:
the first attempt at this item added a unicast-SSDP probe to `tv-rs-discovery`, which is a
different mechanism for a question already answered, and was reverted rather than shipped
unused. A unicast SSDP probe *would* add something the facade's vendor-by-vendor probe
cannot (it could name a device whose HTTP APIs are unreachable), but nothing needs that
today, so it is a note rather than code.

**The D list is empty of buildable items, and that is a finding rather than a
completion.** Each remaining bullet was checked against the code before this was written,
and three of the four were already done:

- **G16 — DIAL launching: DONE.** The item's own "decided shape" — *expose the DIAL
  capability as its own operation, taking an app id and a payload* — is exactly what
  exists: `build_dial_launch_url` (`crates/tizen/src/client.rs:234`),
  `TizenClient::dial_launch(app_id, payload)` (line 625), `launch_with_payload` as a
  **public trait method** (`crates/core/src/capability.rs:422`), and the facade routing
  Tizen + payload to DIAL (`crates/tv-rs/src/handle.rs:348`) — whose doc comment already
  states that `open_url` stays untouched because a set with no browser has no browser.
  Nothing to build.
- **G17 — DLNA transport info: DONE.** `get_transport_info` and `get_position_info` exist
  as free functions (`crates/dlna/src/soap.rs:292`/`301`) *and* as `AvTransport` trait
  methods (lines 394/396). The audit's "neither action appears in the crate" was false.
- **G18 — event push: open, but smaller than the audit said.** The audit claimed "the only
  `subscribe` in the tree is the agent's MQTT transport". False: `tv-rs-dlna` carries a
  complete GENA surface — `subscribe_renderer`, `renew`, `unsubscribe`, a
  `SubscriptionRegistry` with renew-failure accounting, `parse_notify_body`, and
  `TransportEvent`/`TransportState`. What is genuinely missing is the **facade** push API,
  and a design question sits in front of it: DLNA renderers are not part of `TvClient`
  (which holds Tizen/webOS/Roku plus the erased MDC handle), so a push surface must decide
  whether a renderer becomes a fourth arm or a separate subscription handle. **That
  decision is the work, not the plumbing — and it is now decided: a separate handle.**

  **Decision: a renderer does not become a `TvClient` arm.** The precedent is G10e, which
  hit the identical question for MDC and answered it the same way: `TvVendor` has no MDC
  variant and the facade exposes `mdc_handle()` returning an erased handle, because a
  panel that is not a television should not be smuggled into the television enum. A DLNA
  MediaRenderer is not a television either — it is a device you cast *to*, with no power,
  no input and no key surface — so the push API is a subscription handle over
  `gena_client::subscribe_renderer`, not a fourth arm. Two further reasons this is the
  cheaper answer: `TvClient`'s arms each carry a vendor client *and* a connection pool,
  neither of which a renderer has, and `DeviceHandle` already exists for the
  many-devices case the companion cares about.

  **What that leaves to build:** a `SubscriptionHandle` (or equivalent) that owns a
  `SubscriptionRegistry` tick loop, surfaces `TransportEvent`s to a caller as they arrive
  rather than on poll, and renews before expiry — the registry already tracks the
  renew-failure budget, so the missing part is the delivery side, not the bookkeeping.

**The pattern is now four for four, and it is the most useful thing this page has
produced.** G15, G16 and G17 were all reported open and all three were already
implemented; G18 was reported smaller than it is. Every one failed the same way: a grep
for the identifier the plan used, concluding "absent" when the *behaviour* existed under
another name or in another crate. The rule that follows is in the
`docs-source-reconciliation` skill — **before implementing a plan item, grep for the
capability, not the signature** — and it is why this page now records what was checked
next to each item rather than only what remains.

### 2.4 X · Android TV as a target — the spike that decides (not a crate yet)

The matrix deferred Android TV for ADB's trust model (developer mode, per-TV
pairing). The peer landscape ([`03-matrix.md`](03-matrix.md#peer-landscape--the-same-work-in-other-languages))
researched 2026-10-23 found a second door: the **Android TV Remote protocol v2** —
the same protocol the Google TV remote app speaks, no developer mode, pre-installed
`Android TV Remote Service`, mTLS with the Polo pairing handshake on fixed
6467/6466, protobuf commands, and the device's **name + MAC parseable from the
server certificate** on the pairing port (no pairing needed to read it — that is
what G4 needs). Proven viable in Python (`androidtvremote2`, maintained).

**Fact source (decided):** the protocol brief crystallized with the deep-research
skill, `androidtv-v2/references/atr-v2-brief.md`, carries the wire shapes, the
capability truth table, the probe script (9 ordered probes against one panel),
and the effort estimate (~5–7 dev-days spike-quality). Its research verdict:
ATR v2 as primary transport, ADB as privileged fallback (Reboot + arbitrary
intents), WOL/CEC wake ladder for deep-sleep panels.

**Shape (spike):** run the brief's 9 probes against a real panel — no REA
session is planned: its residual unknowns are firmware-runtime behaviors a
decompile cannot answer. Record a probe row per gate in `08-hil-results.md` and
write the verdict here. No `tv-rs-androidtv` crate before the verdict ships and
no re-deferral of A5/C1 — the phone-side sender work rides its own plan items.

**Exit:** `08-hil-results.md` has the ATR v2 probe rows (or a dated "not reachable"
row); the playbook's Android TV paragraph reflects the verdict; the matrix
deferred list is updated the day a crate is decided either way.

## 2.5 Room Control Expansion — PJLink, MDC Serial, and CEC Topology

Goal: Extend control off the local host and into the room via PJLink (Projectors), MDC (Video Walls), and HDMI-CEC Logical Address Topology.

### V1 · PJLink Protocol (`tv-rs-pjlink`)
Pure Rust TCP (port 4352) and RS-232 codec for Class 1 & Class 2 projectors. Maps projector commands to `tv_rs_core` abstractions (`PowerControl` for `%1POWR=1` power commands, `PanelHoursProvider` / LFD lamp queries for `LAMP?` parsing, and MD5 challenge-response authentication).

- **Concept & Domain Mapping**: The crate (`tv-rs-pjlink`, living in `crates/pjlink`) implements `DeviceHandle` and core capability traits (`PowerControl`, `PanelHoursProvider`, `InputSelect`).
- **Falsifiable Gate**: Round-trip unit and integration tests against a `MockPjlinkServer` in `tv-rs-sim`. Asserts `%1POWR=1` (Power On execution) and `LAMP?` response parsing into `PanelHoursProvider` lamp runtime hours and status.

### V2 · MDC Serial Expansion (`tv-rs-mdc`)
Refactor and extend `tv-rs-mdc` to leverage the workspace Serial HAL (`tv-rs-serial` / `tokio-serial`) for RS-232 daisy-chaining across video wall displays using MDC display IDs (`0x00`–`0xFD` individual, `0xFE` broadcast).

- **Concept & Domain Mapping**: Facade (`mdc_serial_handle`) and MDC transport route commands over serial ports, supporting multi-drop display addressing and frame framing over physical or virtual RS-232 connections.
- **Falsifiable Gate**: The facade routes MDC commands over a mock serial port / `MockMdcServer` to a multi-display video wall fixture asserting correct display ID addressing and checksum validation.

### V3 · CEC Logical Address Topology (`tv-rs-cec`)
Extend `tv-rs-cec` to map HDMI-CEC logical addresses (0–15: TV, Recording Device, Tuner, Playback Device, Audio System, etc.) to domain entities (`DeviceDescriptor`, `DeviceHandle`, input routing).

- **Concept & Domain Mapping**: Maps logical addresses to domain endpoints for input routing and status queries. Audio commands (`<User Control Pressed>` volume/mute) are explicitly dropped or forwarded based on the target logical address type and capability set.
- **Falsifiable Gate**: Mock CEC adapter receives `<Give Device Power Status>` (`0x8F`) and replies with the observed domain power state (`<Report Power Status>`), with test assertions verifying audio command forwarding and dropping rules according to domain topology.

## 3. `tv-rs-sim` — verify the hardware exits without hardware

Every exit above that says "on a real TV" needs something that answers like one. This
is a fake panel that speaks the **real** protocols — Tizen remote channel (TLS,
token prompt), webOS SSAP, Roku ECP, MDC 1515, DLNA — so the tests that matter run in
CI: connection reuse (A1), pairing vs token expiry (A2), wake (A4), key input (B1).

It is a test double, so it is `publish = false` until something outside this
workspace wants it, and it never claims a capability a real panel lacks — the whole
point is that its answers are the honest ones.

**The order is by what HTTP mocking cannot reach, and the first draft of this list had
it backwards.** `wiremock` is already a workspace dependency and already mocks the
HTTP vendors — `crates/roku`, `crates/tizen` and `crates/agent` all use it (the Roku
tests assert `.and(path("/keypress/PowerOff"))`). A Roku ECP simulator would therefore
**duplicate existing coverage**, and it was originally written first here on the reasoning
"cheapest protocol" — which is exactly the wrong criterion. The sim earns its place only
where `wiremock` cannot follow:

1. **MDC — start here.** TCP 1515 with length-prefixed binary frames: no HTTP, so nothing
   in the tree mocks it today. It is also the crate with a real frame parser, so the sim
   doubles as a parser fixture and the first slice needs no certificate story.
2. **webOS (SSAP)** — a WebSocket, which `wiremock` does not do, and the first slice that
   needs a registration/pairing prompt — which is what the A2 exit actually wants proven.
3. **Tizen** — the remote channel is TLS *and* a token prompt, so it needs a self-signed
   certificate and the pairing flow. Last of the three because its own complexity starts to
   rival the code it tests.
4. **DLNA** — SSDP multicast plus a SOAP endpoint. Belongs with G18's work rather than
   ahead of it, since the subscription surface is what would use it.
5. **Roku (ECP)** — **not a slice.** Plain HTTP on 8060, already covered by `wiremock`.
   Adding it would be a second mechanism for a solved problem, which is the mistake this
   plan already records twice (G15's unicast probe, and the DIAL manifest claim).

Each slice must answer with the **real** shapes from
[`02-playbook.md`](02-playbook.md) — the point is that the sim's answers are the honest
ones, so a shape the playbook does not have is a shape the sim must not invent. That makes
the playbook the spec for this crate, and any gap found while building a slice is a
playbook finding first.

**Exit:** `cargo test` covers the A1 and A2 exits with no hardware; the sim's
transcripts are checked against the protocol facts in
[`02-playbook.md`](02-playbook.md); a doc records which exits are now CI-verified and
which still need a panel.

## 4. `tv-rs-sony` — DROPPED

**Removed at the owner's instruction** ("certainly not sony"). It is not deferred and not
queued: it is out of scope for this workspace. Recorded rather than deleted so the next
reader does not rediscover the Bravia REST + IRCC research and re-add it as an open item —
Appendix A still lists `tv-rs-bravia` among its port candidates, and that row is marked
dropped. Sony Bravia is not a candidate; the rest of Appendix A is.

## 5. Native CEC — only after a measurement

`tv-rs-cec` shells out to `cec-client`. That is a deliberate trade (no `unsafe`, no C
toolchain), and it is fine until someone shows the subprocess latency hurts. If that
measurement exists, add a `cec-native` feature that talks to `/dev/cec` with `unsafe`
confined to one module; keep the subprocess adapter as the default.

**Exit:** a number, not an opinion — p50/p95 for a standby command through each path,
and no new botched error paths.

## 6. DDC/CI in `tv-rs-local`

Brightness and input for the display the machine is _attached to_, over `/dev/i2c`
(`VCP 0x10`, `VCP 0x60`). It belongs in `tv-rs-local` because that crate already owns
"this host's panel", and it must never be confused with controlling a remote panel.

**Exit:** Linux-only, feature-gated, reads VCP codes on a real monitor, and returns a
typed error where i2c is unavailable.

## 7. Hardware verification — the only unmet gate

`cargo xtask hw-test --write-doc` fills [`08-hil-results.md`](08-hil-results.md) the
moment a television answers SSDP. Until then every protocol claim in this repository is
documented-and-mocked.

**Exit:** at least one row per supported vendor (Tizen, webOS, Roku, MDC), each stating
what was commanded and what came back — or an explicit "not verified" line, which is
also a result.

## 8. Local media caching and multi-screen sync

`tv-rs-media` serves a remote panel's stream. The same node should cache to local disk
and drive several panels from one producer without pulling the source through the
network twice: a player-level cache plus a sync coordinator on `tv-rs-media`.

**Exit:** a local cache hit is measurable and reportable; two-panel sync shows no double
network transfer for a shared source; no regression in the existing `Media` traits.

## 9. Release `0.4.0`

Every open item above, shipped in the order of this page: `tv-rs-core`, protocol
crates, `tv-rs-mdc`, then the agent, media and facade. Under the policy in
`CONTRIBUTING.md` the additions are a **minor** bump.

**Exit:** all fifteen publishable crates published at the same version;
[`08-hil-results.md`](08-hil-results.md) has at least one real row; the vendor capability
table ([`03-matrix.md` §Capability truth](03-matrix.md#capability-truth)) states each
vendor's absence rather than implying a capability it lacks.

---

# Appendix A — Expansion proposal: industry ports and new layers

**A proposal, not a commitment.** Nothing here is queued; the queued work is items 1–9
above. It was written on 2026-10-23 and is kept for the research in it — the port table,
the crates worth wrapping, the layer sketch and the risks. A row here becomes an item
above only when it passes the [hardware-first rule](#hardware-first-then-publish).

## Summary

tv-rs becomes the most complete open signage control stack by adding three things it lacks: more display protocols, an edge-host runtime, and a mobile companion core. All of it stays pure Rust, with no desktop UI (the existing Tauri platform covers that) and no database (state lives in files and in the devices themselves).

Today tv-rs covers Samsung (Tizen, MDC), LG webOS, Roku, DLNA, Wake-on-LAN, SSDP, HDMI-CEC, an HLS/`206` media server, a host display probe and a TV app producer. The gaps that matter most for signage:

- **Commercial displays beyond Samsung:** Philips SICP, LG commercial RS-232/IP, Sharp/NEC, projectors over PJLink.
- **Consumer platforms:** Android TV / Google TV, Cast, Apple TV, Philips JointSPACE, Hisense VIDAA, Vizio SmartCast.
- **Host hardware:** DDC/CI for monitors, native CEC, IR blasters, GPIO and sensors, display output control.
- **Edge player runtime:** playback, sync across a video wall, proof-of-play, watchdog, offline content cache.
- **Mobile companion:** a shared Rust core for pairing, provisioning and LAN control, exposed to Swift and Kotlin.

The proposal adds about 15 crates in four phases. Commercial-display protocols come first because they are small, documented byte protocols and the highest-value gap for signage.

## Port candidates

The best ports are small wire protocols with a public spec and a reference implementation in Python, C# or JS. Port the protocol from the spec and use the reference library only to check behaviour, so its licence doesn't carry over. Port numbers, licences and maintenance status are approximate and were not verified page by page; confirm each against the vendor spec before building.

### Commercial displays and AV (highest value)

| Target | Reference implementation | Wire protocol | Effort | Proposed crate |
| --- | --- | --- | --- | --- |
| Philips signage | [philips-signage-display-sicp](https://github.com/aolde/philips-signage-display-sicp) (C#, also on NuGet) | SICP: binary frames, checksum, TCP 5000 or RS-232 | Low, same shape as MDC | `tv-rs-sicp` |
| LG commercial | [libLGTV\_serial](https://github.com/ehjortberg/libLGTV_serial), [lg-signage](https://www.npmjs.com/package/lg-signage) (npm), [openHAB LG serial binding](https://www.openhab.org/addons/bindings/lgtvserial/) | ASCII commands (`ka 01 01`), RS-232 or TCP 9761 | Low | `tv-rs-lgcmd` |
| Sony Bravia Pro | [Sony Simple IP Control spec](https://pro-bravia.sony.net/remote-display-control/simple-ip-control/), [sonybraviatv-remotecontrol](https://pypi.org/project/sonybraviatv-remotecontrol/) | Fixed 24-byte ASCII frames on TCP 20060, plus a REST/JSON API | Low | **`tv-rs-bravia` — dropped ([item 4](#4-tv-rs-sony--dropped)), not a candidate** |
| Sharp/NEC LFD | [nec-pd-sdk](https://pypi.python.org/pypi/nec-pd-sdk) (Python), [external control manual](https://assets.sharpnecdisplays.us/documents/usermanuals/external-control-e988.pdf) | NEC binary protocol, TCP 7142 or RS-232 | Medium, larger command set | `tv-rs-necpd` |
| Projectors | [pypjlink](https://pypi.org/project/pypjlink/), [aiopjlink](https://pypi.org/project/aiopjlink) | PJLink Class 1/2, TCP 4352, ASCII, MD5 auth | Low | `tv-rs-pjlink` |

### Consumer platforms

| Target | Reference implementation | Wire protocol | Effort | Proposed crate |
| --- | --- | --- | --- | --- |
| Android TV / Google TV | [androidtvremote2](https://github.com/tronikos/androidtvremote2) (Python) | Remote protocol v2: protobuf over TLS, pairing on 6467, control on 6466 | Medium | `tv-rs-androidtv` |
| Android devices (sideload, shell) | [adb\_client](https://www.lib.rs/crates/adb_client) already exists in Rust | ADB over TCP 5555 | Wrap, don't port | inside `tv-rs-androidtv`, feature `adb` |
| Chromecast / Cast | [rust\_cast](https://docs.rs/crate/rust_cast) already exists in Rust | Cast v2: protobuf over TLS 8009 | Wrap, don't port | `tv-rs-cast` |
| Apple TV | [pyatv](https://pypi.org/project/pyatv/) (Python) | Companion, AirPlay 2, MRP; HAP-style pairing | High, crypto-heavy | `tv-rs-appletv` (later) |
| Philips consumer | [ha-philipsjs](https://pypi.org/project/ha-philipsjs/) | JointSPACE REST, digest auth on 1926 | Low | `tv-rs-jointspace` |
| Hisense VIDAA | [hisensetv](https://hisensetv.readthedocs.io/) | MQTT on the TV, port 36669 | Low, reuse `rumqttc` | `tv-rs-hisense` |
| Vizio | [pyvizio](https://github.com/raman325/pyvizio); a small [smartcast](https://docs.rs/crate/smartcast/latest) Rust crate exists | SmartCast REST over HTTPS 7345/9000, PIN pairing | Low | `tv-rs-vizio` |

### Hardware around the screen

| Target | Reference implementation | Wire protocol | Effort | Proposed crate |
| --- | --- | --- | --- | --- |
| IR blasters | [python-broadlink](https://docsearch.algolia.com/mcp/docs/repo/mjg59/python-broadlink) | Broadlink UDP, AES-CBC payloads | Medium | `tv-rs-ir` |
| Monitors | [ddc-hi](https://docs.rs/ddc-hi) already exists in Rust | DDC/CI (MCCS VCP codes) over I²C | Wrap | `tv-rs-ddc` |
| HDMI-CEC (native) | [cec-rs](https://lib.rs/crates/cec-rs) bindings to libcec | CEC via libcec | Wrap, replaces the `cec-client` subprocess | upgrade `tv-rs-cec` |
| Wi-Fi provisioning | [Improv](https://improv.hexdocs.pm/readme.html) spec; [improv-wifi](https://docs.rs/improv-wifi) crate | BLE GATT or serial | Low | `tv-rs-provision` |

Also worth mining for behaviour, not code: Home Assistant integrations (quirks per model) and control-system driver stores such as RTI and Crestron, whose help PDFs document many command sets.

## Rust crates to reuse instead of porting

Wrap these behind tv-rs traits rather than rewriting them. Each wrapper is an optional feature, so the default build stays small.

| Need | Crate | Use in tv-rs |
| --- | --- | --- |
| Cast v2 control | [rust\_cast](https://docs.rs/crate/rust_cast) | `tv-rs-cast` backend implementing `DeviceHandle` |
| ADB over TCP | [adb\_client](https://www.lib.rs/crates/adb_client) | sideload and shell on Android-based panels |
| mDNS / DNS-SD | [libmdns](https://lib.rs/crates/libmdns), [simple-mdns](https://docs.rs/simple-mdns) | discover Cast, AirPlay, Android TV; advertise the edge host |
| DDC/CI | [ddc-hi](https://docs.rs/ddc-hi) | brightness, input, power on monitors attached to the host |
| HDMI-CEC | [cec-rs](https://lib.rs/crates/cec-rs) | native CEC, replacing the `cec-client` subprocess |
| Precise time | [statime](https://tweedegolf.nl/en/blog/72/announcing-statime-a-rust-ptp-implementation) (PTP) | frame-accurate sync across a video wall |
| Wi-Fi provisioning | [improv-wifi](https://docs.rs/improv-wifi) | first-boot setup from the phone |
| Playback | gstreamer-rs or libmpv bindings | the edge player's media pipeline |
| Display output | `drm` / KMS crates | mode setting, rotation, blanking on bare Linux |
| GPIO, I²C, sensors | `rppal`, `embedded-hal`, `linux-embedded-hal` | presence sensors, relays, panel temperature |
| BLE | `btleplug` | phone pairing, beacons, provisioning |
| Serial | `tokio-serial` (already used) | RS-232 for SICP, LG, NEC, MDC |
| MQTT | `rumqttc` (already used) | fleet messages, Hisense control |
| Mobile bindings | `uniffi` | expose the companion core to Swift and Kotlin |
| Firmware | `embassy`, `esp-hal` | optional microcontroller sidecar |

## Proposed layers

The new work sits in four layers around the existing core. Each crate implements the
existing `DeviceHandle` and capability traits, so the agent, the CLI and a Tauri platform
pick it up with no new code paths.

```
  phone ─────▶ edge host ─────▶ displays ─────▶ hardware
 (uniffi)      (hostd)          (protocols)     (ddc / cec / ir / gpio)
                    │                │                  │
                    └────────────────┴──────────────────┘
                                     ▼
                          tv-rs-core (DeviceHandle, capabilities)
```

The phone talks to the edge host, the host drives displays through protocol and hardware crates, and everything below the host speaks the core's traits.

### 1. Protocol layer: more displays

- One crate per wire protocol from the port table: `sicp`, `lgcmd`, `bravia`, `necpd`, `pjlink`, `androidtv`, `cast`, `jointspace`, `hisense`, `vizio`.
- A shared `tv-rs-serial` helper so every RS-232 protocol gets the same framing, timeouts, retries and RS-485 multi-drop addressing.
- One conformance run per crate against the existing mock and conformance suite.

### 2. Hardware layer: what is wired to the host

- `tv-rs-ddc`: DDC/CI for monitors (brightness, input, power, VCP read-back).
- `tv-rs-cec` v2: native libcec, CEC event subscriptions, multiple adapters.
- `tv-rs-ir`: IR blaster backends (Broadlink, plus GPIO IR on a Pi), with a learned-code store in TOML.
- `tv-rs-gpio`: relays (hard power cycle), presence sensors, ambient light for auto-brightness, temperature.
- `tv-rs-local` v2: display output control on Linux via DRM/KMS: modes, rotation, blanking.

### 3. Edge host layer: the box behind the screen

- `tv-rs-player`: a playback runtime over GStreamer or libmpv, with playlists as data, gapless switching and hardware decode. It plays, it does not schedule.
- `tv-rs-sync`: PTP or NTP clock discipline plus a start-at-timestamp protocol, so walls and multi-screen sites play in lockstep.
- `tv-rs-cache`: an offline content cache with content-hash verification, resumable downloads and size limits.
- `tv-rs-pop`: proof-of-play events and screenshots, written as JSONL like the existing audit log.
- `tv-rs-watchdog`: health checks, systemd notify and the hardware watchdog, reusing the agent's healing ladder.
- `tv-rs-hostd`: one daemon binary that wires the above to the agent, advertises itself over mDNS and serves a small local API for the phone.

### 4. Companion layer: the phone app's core

- `tv-rs-companion`: a pure-Rust core with no UI, exposed to Swift and Kotlin through `uniffi`.
- It handles discovery (mDNS plus SSDP), pairing flows for each vendor, remote keys, power and input, and talking to `tv-rs-hostd`.
- `tv-rs-provision`: first-boot setup over BLE using Improv, then QR-code pairing between phone and host.
- Platform constraints to design around: iOS needs the multicast networking entitlement, and Android needs a multicast lock for SSDP and mDNS.

No layer adds a database. Desired state, pins, tokens and caches stay as files, matching how tv-rs already stores pairing tokens and quirks.

## Roadmap

Build in four phases, ordered by signage value and by how cheaply each can be proven on real hardware.

```
  Phase 1  commercial protocols   (sicp, lgcmd, necpd, pjlink)
  Phase 2  host hardware          (ddc, cec v2, ir, gpio, local v2)
  Phase 3  edge host runtime      (player, sync, cache, pop, watchdog, hostd)
  Phase 4  mobile companion       (companion core, provision)
```

Each gate is a hardware test, not a date: the next phase starts only when the previous one has verified rows in [`08-hil-results.md`](08-hil-results.md). Apple TV and other heavy consumer backends come after phase 4.

## Risks and design rules

The main risk is breadth without hardware proof: tv-rs already has zero hardware-verified rows, and 15 more crates would widen that gap.

- <a id="hardware-first-then-publish"></a>**Hardware first, then publish.** No new protocol crate ships to crates.io until it has at least one row in [`08-hil-results.md`](08-hil-results.md). Buy or borrow one panel per protocol (a used Philips, LG commercial and NEC panel each cost little second-hand).
- **Port from specs, not code.** Treat Python and C# libraries as behaviour references. This avoids inheriting a reference library's licence and keeps the tv-rs style intact.
- **Keep native code optional.** libcec, GStreamer and libmpv pull C libraries. Each stays behind a feature flag, and the pure-Rust paths (SICP, PJLink, LG, NEC) need no system libraries.
- **Apple TV is a cost trap.** The Companion and AirPlay 2 pairing crypto is large and moves with Apple's OS releases. Defer it until the other consumer backends are done.
- **Vendor changes break consumer protocols.** Android TV, Vizio and Hisense have changed pairing before. Route model and firmware defects through the existing quirks file, not code branches.
- **Mobile multicast limits.** iOS discovery needs Apple's multicast entitlement approval; plan a fallback where the edge host does discovery and the phone talks only to the host.
- **Bus factor.** The repo has one maintainer. A clear crate-per-protocol layout and the conformance suite are what make outside contributions safe; write `CONTRIBUTING` steps for adding a protocol before opening the work up.

## Sources

Found through web search on 2026-10-23; the pages were not opened individually, so treat details as leads to confirm.

- [tv-rs repository](https://github.com/Zakarialabib/tv-rs)
- [androidtvremote2](https://github.com/tronikos/androidtvremote2)
- [pyatv](https://pypi.org/project/pyatv/)
- [rust\_cast](https://docs.rs/crate/rust_cast)
- [philips-signage-display-sicp](https://github.com/aolde/philips-signage-display-sicp)
- [ha-philipsjs](https://pypi.org/project/ha-philipsjs/)
- [Sony Bravia Simple IP Control](https://pro-bravia.sony.net/remote-display-control/simple-ip-control/) (dropped target)
- [nec-pd-sdk](https://pypi.python.org/pypi/nec-pd-sdk)
- [Sharp/NEC external control manual](https://assets.sharpnecdisplays.us/documents/usermanuals/external-control-e988.pdf)
- [libLGTV\_serial](https://github.com/ehjortberg/libLGTV_serial)
- [lg-signage (npm)](https://www.npmjs.com/package/lg-signage)
- [pypjlink](https://pypi.org/project/pypjlink/), [aiopjlink](https://pypi.org/project/aiopjlink)
- [pyvizio](https://github.com/raman325/pyvizio)
- [hisensetv](https://hisensetv.readthedocs.io/)
- [python-broadlink](https://docsearch.algolia.com/mcp/docs/repo/mjg59/python-broadlink)
- [ddc-hi](https://docs.rs/ddc-hi)
- [statime](https://tweedegolf.nl/en/blog/72/announcing-statime-a-rust-ptp-implementation)
- [libmdns](https://lib.rs/crates/libmdns)
- [adb\_client](https://www.lib.rs/crates/adb_client)
- [cec-rs](https://lib.rs/crates/cec-rs)
- [Improv Wi-Fi](https://improv.hexdocs.pm/readme.html), [improv-wifi crate](https://docs.rs/improv-wifi)
