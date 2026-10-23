# Plan

**This is the only plan.** It holds open work and nothing else: an item exists here
only while it is **not** done, and when it lands its section is deleted. Landed work is
[`CHANGELOG.md`](../CHANGELOG.md); what each crate implements today is
[`03-matrix.md`](03-matrix.md); why the architecture is what it is is
[`architecture.md`](architecture.md) and the [ADRs](adr/). This page never grows a
history.

---

## 1. Release `0.3.0` — blocked on a token

The version bump, the lockfile, the ordered publish and every gate are done. The first
upload is refused `403 Forbidden`: the available token is `publish-new`, and updating
the thirteen existing crates needs `publish-update` (`tv-rs-mdc` alone would not, since
it is new). Nothing else stands between `main` and this release.

Everything on `main` ships in this release, which is why it is a minor bump: the
breaking additions are `Remedy::Quarantine`, `DeviceOs::SamsungMdc`, the three
telemetry capabilities, and the five LFD capability variants (`PanelLock`,
`Orientation`, `NoSignalStatus`, and behind `supporting` `LampOrPanelHours` and
`ScreenCapture`).

`cargo xtask publish --version 0.3.0` rewrites every pin and publishes in dependency
order — `tv-rs-core` first, then the leaves (`tv-rs-mdc` and the new `tv-rs-app`
among them), then the crates that name them. The order is data, not prose:
`xtask/src/main.rs::ORDER` is the single home, and
[ADR-001](adr/001-crate-publication.md) records why it is shaped that way.

**A note on the dry run.** `cargo xtask publish --version 0.3.0 --dry-run` verifies only
the first crate. `cargo publish` resolves a tarball's dependencies from the registry,
and the other fourteen crates depend on `0.3.0` siblings that are not on it yet — so
the run stops at crate two with `candidate versions found which didn't match`. That is
the release's state, not a defect: the ordered upload is what makes the real run work,
because each crate is on the index before the next one resolves it. It is also why
`ORDER` exists.

**Exit:** fifteen crates at `0.3.0`, a fresh `cargo add tv-rs` consumer builds against
`power_observation()` and `assess_telemetry`, docs.rs serves the new items, and the
CHANGELOG and README agree with the registry.

## 2. `tv-rs-app` — the TV app producer (SignageMaster handoff E6)

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

## 2b. Display probe merge (SignageMaster handoff E3b) — the consumer switch

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
in its `win32.rs` are output *control*, which is [item 7](#7-ddcci-in-tv-rs-local)
and the expansion plan's `tv-rs-local` v2 — not part of the probe.

**Exit:** SignageMaster depends on `tv-rs-local` for EDID identity and deletes
`edid.rs`; this section is deleted.

## 3. Companion-app gaps — the tv-rs side

Requested by the companion app (`companion-app/docs/TV-RS-GAPS.md`, ranked by its
`BACKEND-PLAN.md`). Every gap below was verified open on `main` before it was written
down. **One PR per gap, with its test and a CHANGELOG entry; the section is deleted
when it lands.** The gap work ships in [0.4.0](#10-release-040).

Tier A shapes the contract, so it goes before the companion writes its `src-tauri`.
Tier B makes the Remote real, per vendor — reorder it by the TVs on the bench.
A2 landed on 2026-10-23 (one error type, exhaustive remedy maps, the webOS
registration split); its record lives in [CHANGELOG.md](../CHANGELOG.md).

### 3.2 A1 · G11 — sessions are not reused

`impl KeyInput for TvClient` calls `TizenSession::connect` on every key press — TCP,
TLS and the `ms.channel.connect` handshake each time (300–1500 ms), while
`TizenConnectionPool` sits unused; webOS connects per request too.

**Exit:** the facade owns the pools (Tizen's existing pool plus a webOS session
pool), idle timeout ~60 s, one reconnect on error, and a public `close_all()` for
mobile backgrounding — proven by [`tv-rs-sim`](#4-tv-rs-sim--verify-the-hardware-exits-without-hardware):
twenty key presses, one accepted connection.

### 3.3 A3 · G1 + G2 — serializable core types

No core type is `Serialize`, so every consumer hand-writes DTOs; `ObservedField`
holds a monotonic `Instant`, which cannot be serialised at all.

**Shape (decided):** an optional `serde` feature, off by default (non-serde users pay
nothing), `rename_all = "snake_case"`, `Key::Digit(n)` → `"digit_n"`, and
`ObservedField` serialising **both** `verified_at_ms` and `age_ms` so a UI can show an
age or a timestamp. The wire format is a published contract: documented in the crate
README.

**Exit:** the core types round-trip as JSON; `tv_rs::TvVendor` serialises as
`"samsung"`, not `"Samsung"`; a test pins the wire shape of every serialisable type.

### 3.4 A4 · G13 + G4 — nothing can wake

`wake()` returns `Unsupported(Wake)` for every vendor, `RokuClient::power_on` exists
and is unused, and `DiscoveredTv` carries no MAC, so no `WolSender` can be built.

**Exit:** Roku wakes via ECP `PowerOn`; the MAC is learned while the TV is awake
(Samsung `:8001/api/v2/` `wifiMac`, Roku device-info, webOS `getSystemInfo`) and kept
in `DeviceDescriptor::mac`; `wake()` with a MAC sends a magic packet; each failure
carries the vendor's own hint ("Fast TV start", "Turn on via Wi-Fi", "Power On with
Mobile").

### 3.5 A5 · G14 — an Android block must not read as "TV off"

From Android 17 (target SDK 37) LAN sockets need runtime `ACCESS_LOCAL_NETWORK`:
TCP connects time out and UDP sends fail `EPERM`, both mapped today to
`Timeout`/`Unreachable` → `Retry`/`Wait`, so the UI offers _Wake_ forever.

**Shape (decided):** `TransportError::Blocked { reason }` plus a new
`Remedy::RequestPermission` — the UI must say "grant access", not "wake".

**Exit:** on `target_os = "android"`, `EPERM` and the NDK's
`ANDROID_NETWORK_BLOCKED_REASON_LNP` map to `Blocked`; zero datagrams received is
distinguishable from zero TVs answered; desktop paths are untouched.

### 3.6 B1 · G10a — webOS key input

Without the pointer/input socket (`getPointerInputSocket`) the LG D-pad is dead.

### 3.7 B2 · G10b — foreground on Roku and Tizen

Roku `/query/active-app`, Tizen's foreground app: the "on screen" card and the
fleet's app check need them.

### 3.8 B3 · G5 — installed apps on Tizen and webOS

webOS `listLaunchPoints`; Tizen `ed.installedApp.get` over the remote channel. Today
only Roku lists apps.

### 3.9 B4 · G10c — mute read, and input select

`VolumeControl` has `volume()` and `set_mute()` but no `muted()`; input select is
missing on webOS (`switchInput`) and Tizen (at least `KEY_HDMI1..4`).

### 3.10 B5 · G6 — text entry

Tizen `ms.remote.control` `SendInputString` (base64), webOS IME `insertText`, Roku
`Lit_<char>` key presses — for search boxes on the TV.

### 3.11 C1 · G7 — Android readiness

A distinct error when **zero datagrams** were received (no multicast permission) as
opposed to zero TVs answering; a `rustls-platform-verifier` feature for
`tv-rs-transport`; `cfg`-gate `tv-rs-local` so Android and iOS do not build a
desktop EDID probe.

### 3.12 D — the rest, in the companion's order

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

### 3.13 X · Android TV as a target — the spike that decides (not a crate yet)

The matrix deferred Android TV for ADB's trust model (developer mode, per-TV
pairing). The peer landscape ([`03-matrix.md`](03-matrix.md#peer-landscape))
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

## 4. `tv-rs-sim` — verify the hardware exits without hardware

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

## 5. `tv-rs-sony` — Bravia REST + IRCC

Sony's consumer and pro panels expose a documented JSON-REST API plus IRCC-IP for key
presses, authenticated with a pre-shared key. It is the smallest remaining vendor
integration: power, input, volume, app launch, and key codes.

**Exit:** passes the conformance suite; capability set is honest (no absolute volume if
the API only takes steps); pairing failure maps to `Auth` with `Remedy::Reprovision`.

## 6. Native CEC — only after a measurement

`tv-rs-cec` shells out to `cec-client`. That is a deliberate trade (no `unsafe`, no C
toolchain), and it is fine until someone shows the subprocess latency hurts. If that
measurement exists, add a `cec-native` feature that talks to `/dev/cec` with `unsafe`
confined to one module; keep the subprocess adapter as the default.

**Exit:** a number, not an opinion — p50/p95 for a standby command through each path,
and no new botched error paths.

## 7. DDC/CI in `tv-rs-local`

Brightness and input for the display the machine is _attached to_, over `/dev/i2c`
(`VCP 0x10`, `VCP 0x60`). It belongs in `tv-rs-local` because that crate already owns
"this host's panel", and it must never be confused with controlling a remote panel.

**Exit:** Linux-only, feature-gated, reads VCP codes on a real monitor, and returns a
typed error where i2c is unavailable.

## 8. Hardware verification — the only unmet gate

`cargo xtask hw-test --write-doc` fills `docs/hil-results.md` the moment a television
answers SSDP. Until then every protocol claim in this repository is
documented-and-mocked.

**Exit:** at least one row per supported vendor (Tizen, webOS, Roku, MDC), each stating
what was commanded and what came back — or an explicit "not verified" line, which is
also a result.

## 9. Local media caching and multi-screen sync

`tv-rs-media` serves a remote panel's stream. The same node should cache to local disk
and drive several panels from one producer without pulling the source through the
network twice: a player-level cache plus a sync coordinator on `tv-rs-media`.

**Exit:** a local cache hit is measurable and reportable; two-panel sync shows no double
network transfer for a shared source; no regression in the existing `Media` traits.

## 10. Release `0.4.0`

Every open item above, shipped in the order of this page: `tv-rs-core`, protocol
crates, `tv-rs-mdc`, then the agent, media and facade. Under the policy in
`CONTRIBUTING.md` the additions are a **minor** bump.

**Exit:** all fourteen crates published at the same version; `docs/hil-results.md` has
at least one real row; the vendor capability table
([`03-matrix.md` §Capability truth](03-matrix.md#capability-truth)) states each
vendor's absence rather than implying a capability it lacks.
