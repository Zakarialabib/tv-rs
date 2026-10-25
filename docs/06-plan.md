# Plan

**This is the only plan.** It holds open work and nothing else: an item exists here
only while it is **not** done, and when it lands its section is deleted. Landed work is
[`CHANGELOG.md`](../CHANGELOG.md); what each crate implements today is
[`03-matrix.md`](03-matrix.md); why the architecture is what it is is
[`architecture.md`](architecture.md) and the [ADRs](adr/). This page never grows a
history.

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
  until [`hil-results.md`](hil-results.md) has a row for them.
- **`.wgt` is a ZIP**, so it is written here. A `.ipk` is an `ar` archive of control
  and data tarballs: assembling one needs the webOS CLI (`ares-package`), so the
  crate emits the webOS app directory and returns a typed error naming that tool
  rather than pretending to have built an installable package.

**Where this stands — and it is done on this side.** The crate exists, is tested
(descriptor validation, both manifests, the bundle, the ZIP writer, the generated bridge
and the typed `.ipk` refusal: 42 tests), the `.wgt` it produces was opened by two
independent readers (Python's `zipfile`, which also verified every CRC, and Windows'
`Expand-Archive`), and **it published in 0.3.0**. The only thing left is the consumer
deleting its own wrapper — which is SignageMaster's work, not this workspace's, and is why
this section is only waiting on a downstream commit rather than on code.

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
and the expansion plan's `tv-rs-local` v2 — not part of the probe.

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
sources in [`01-playbook.md`](01-playbook.md), `learn_mac()` answering on both vendors that
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

**A trace for the id source is in flight** (the same two-source bar as the endpoint above):
does DIAL's `/apps` endpoint exist on Samsung sets and what does it return, does the
device-description XML list apps or only services, what do real clients do to enumerate,
and — if enumeration exists only over the WebSocket control channel — which method. Until
that comes back with two sources, nothing here is implemented.

**A trap this item should not repeat.** `apps.rs`'s comment asserting that the DIAL manifest
"lists what is installed" is itself an unverified claim, and it is the kind that reads as
settled fact. Whoever picks this up should treat it as a lead, not as documentation.

### 2.3 D — the rest, in the companion's order

Each entry was checked against the code before being left standing, because these
premises go stale: what the audit found is recorded next to the item. Landed items are
gone from the list — G3 (streaming discovery), G8 (release from quarantine) and G10e (MDC
behind the facade) all landed, and their rationale now lives in the CHANGELOG and the
glossary.

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
decompile cannot answer. Record a probe row per gate in `hil-results.md` and
write the verdict here. No `tv-rs-androidtv` crate before the verdict ships and
no re-deferral of A5/C1 — the phone-side sender work rides its own plan items.

**Exit:** `hil-results.md` has the ATR v2 probe rows (or a dated "not reachable"
row); the playbook's Android TV paragraph reflects the verdict; the matrix
deferred list is updated the day a crate is decided either way.

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
[`01-playbook.md`](01-playbook.md) — the point is that the sim's answers are the honest
ones, so a shape the playbook does not have is a shape the sim must not invent. That makes
the playbook the spec for this crate, and any gap found while building a slice is a
playbook finding first.

**Exit:** `cargo test` covers the A1 and A2 exits with no hardware; the sim's
transcripts are checked against the protocol facts in
[`01-playbook.md`](01-playbook.md); a doc records which exits are now CI-verified and
which still need a panel.

## 4. `tv-rs-sony` — DROPPED

**Removed at the owner's instruction** ("certainly not sony"). It is not deferred and not
queued: it is out of scope for this workspace. Recorded rather than deleted so the next
reader does not rediscover the Bravia REST + IRCC research and re-add it as an open item —
the expansion proposal ([`08-expansion-plan.md`](08-expansion-plan.md)) still lists
`tv-rs-bravia` among its port candidates, and that entry is a proposal, not a commitment.

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

`cargo xtask hw-test --write-doc` fills `docs/hil-results.md` the moment a television
answers SSDP. Until then every protocol claim in this repository is
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

**Exit:** all sixteen crates published at the same version; `docs/hil-results.md` has
at least one real row; the vendor capability table
([`03-matrix.md` §Capability truth](03-matrix.md#capability-truth)) states each
vendor's absence rather than implying a capability it lacks.
