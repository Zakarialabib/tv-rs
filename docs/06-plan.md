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

**Where this stands.** The crate exists and is tested — descriptor validation, both
manifests, the bundle, the ZIP writer, the generated bridge and the typed `.ipk`
refusal: 42 tests, and the `.wgt` it produces was opened by two independent readers
(Python's `zipfile`, which also verified every CRC, and Windows' `Expand-Archive`).
What is left is the publish, which rides item 1, and the consumer deleting its own
wrapper once the registry has the crate.

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
`edid.rs`; this section is deleted.

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

Wake landed for all three vendors: Roku goes over ECP `PowerOn`, and Tizen and webOS
broadcast a magic packet once a caller supplies the address
(`TvClient::with_mac`), with `Capability::Wake` advertised only where it can actually
be delivered. What is open is the convenience half — learning the address instead of
asking for it.

`TvClient::learn_mac()` answers on Roku (`/query/device-info`, Wi-Fi address preferred
over wired). On Samsung and webOS it refuses, because **neither endpoint this item
originally named has a verified source in this workspace**: the playbook records no
`:8001/api/v2/` `wifiMac` field and no `ssap://system/getSystemInfo` MAC, and the rule
here is that a capability without a verified vendor command does not exist. Refusing
is the honest answer until someone traces one.

**Exit:** a provenance-traced source for each endpoint (added to
[`01-playbook.md`](01-playbook.md) the way the other protocol facts are), `learn_mac()`
returning the address on both vendors, and a test pinning the parsed field.

### 2.2 B2 · G10b (the remainder) — foreground on Tizen

Roku landed: `GET /query/active-app` (verified against Roku's External Control
Protocol documentation, recorded in [`01-playbook.md`](01-playbook.md) §5) parses to
`RokuActiveApp`, the facade's `foreground()` returns the active app's id, and
`Capability::Foreground` is advertised. The home screen reports `Roku` with no `id`,
so it answers `None` — "nothing is in front" — rather than inventing an application.

**Tizen is open, and not because the code is hard:** this workspace has no verified
foreground-app command for the consumer remote channel. The playbook records
`ssap://com.webos.applicationManager/getForegroundAppInfo` for webOS and nothing
equivalent for Samsung, and a capability without a verified vendor command does not
exist here. The candidate (`GET http://<ip>:8001/api/v2/applications`, reading the
entry with `running: true`) needs the same provenance treatment the Roku endpoint just
got — two independent implementations, recorded in the playbook — before it becomes
code.

**Exit:** the endpoint traced and recorded, `TizenClient` returning the foreground app
id, `foreground()` answering on Tizen, and a test pinning the parsed shape.

### 2.3 B5 · G6 (the remainder) — text entry on webOS and Tizen

Roku landed: `keypress/Lit_<char>`, one request per character, traced to Roku's own ECP
documentation and recorded in [`01-playbook.md`](01-playbook.md) §5. The documented
surface is **printable ASCII** — "Printable ASCII character code values can be
transmitted as-is with the `Lit_` prefix" — and the encoding for anything else is not
documented in that source, so `type_text` refuses such a string rather than guessing at
a percent-encoding. `Capability::TextInput` is its own capability (declared after
`AppList`, which is what forced the `CapabilitySet` bitset to widen from `u16` to `u32`)
and the refusal names it, never `Capability::Key`: a device that can press buttons may
still have no way to take a string.

**webOS and Tizen are open, and neither premise is traced.** The plan names webOS IME
`insertText` and Tizen `SendInputString` (base64); a search of the workspace finds no
occurrence of either, and the playbook records no text-entry command for either vendor.
Same rule as every other gap here — a capability without a verified vendor command does
not exist — so both keep the trait default, which names `TextInput` and leaves the
control greyed out rather than silently typing nothing.

**Exit:** the webOS IME surface and the Tizen `SendInputString` payload traced into the
playbook with their sources, then `type_text()` answering on both.

### 2.4 C1 · G7 — Android readiness

A distinct error when **zero datagrams** were received (no multicast permission) as
opposed to zero TVs answering; a `rustls-platform-verifier` feature for
`tv-rs-transport`; `cfg`-gate `tv-rs-local` so Android and iOS do not build a
desktop EDID probe.

### 2.5 D — the rest, in the companion's order

- **G3** streaming discovery (`discover_stream()` or a callback; dedup stays in the
  scanner) — Devices › Find streams results as TVs answer.
- **G8** `HealerLadder::release()` plus an audit entry, for Fleet › "Release from
  quarantine".
- **G10d** DLNA `RenderingControl` (volume/mute for renderers).
- **G10e** MDC behind the facade: `TvVendor::Mdc` and the same `DeviceHandle` path, so
  panels are not a special case.
- **G15** `probe(ip) -> Option<DiscoveredTv>` — "what is at this IP?" for guest Wi-Fi
  and Android pickers that hand back an address.
- **G16** Tizen DIAL URL launching through `TvClient::open_url`.
- **G17** DLNA `get_transport_info` / `get_position_info`, so a cast's position is a
  cold read rather than a session-scoped estimate.
- **G18** event streams (`tv-rs` push instead of request/response) — the companion's
  own list added it while this plan was written; it follows Tier B.

### 2.6 X · Android TV as a target — the spike that decides (not a crate yet)

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

**Exit:** `cargo test` covers the A1 and A2 exits with no hardware; the sim's
transcripts are checked against the protocol facts in
[`01-playbook.md`](01-playbook.md); a doc records which exits are now CI-verified and
which still need a panel.

## 4. `tv-rs-sony` — Bravia REST + IRCC

Sony's consumer and pro panels expose a documented JSON-REST API plus IRCC-IP for key
presses, authenticated with a pre-shared key. It is the smallest remaining vendor
integration: power, input, volume, app launch, and key codes.

**Exit:** passes the conformance suite; capability set is honest (no absolute volume if
the API only takes steps); pairing failure maps to `Auth` with `Remedy::Reprovision`.

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

**Exit:** all fourteen crates published at the same version; `docs/hil-results.md` has
at least one real row; the vendor capability table
([`03-matrix.md` §Capability truth](03-matrix.md#capability-truth)) states each
vendor's absence rather than implying a capability it lacks.
