# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Text entry, Roku half (companion gap G6)**: `RokuClient::type_text()` types a string
  one `keypress/Lit_<char>` request per character, and a new `TextInput` trait (with a
  default that refuses) plus `Capability::TextInput` expose it through the facade —
  `as_text()` and `type_text()`. Source: Roku's own ECP documentation, where the key
  parameter may be "a printable character value specified with the prefix `Lit_`" and
  "Printable ASCII character code values can be transmitted as-is". That surface is
  printable ASCII, so a string holding anything else is refused **atomically** — the
  whole string is validated before the first request, so a refusal never leaves half a
  search term typed into the field. The refusal names `TextInput`, never `Key`, because
  a device that can press buttons may have no way to take a string.
  `Capability::TextInput` is declared last, which forced `CapabilitySet` to widen from
  `u16` to `u32` (the sixteenth capability filled the old width); the widening is
  additive and every capability keeps its index, so a set written by an older build
  reads the same. webOS and Tizen keep the honest default: their text-entry surfaces are
  named in the plan but not traced to a source in this workspace.

- **`AppLaunch::installed_apps()`** — lists what is actually installed, with a
  default of `Unsupported` so a platform that cannot list says so instead of
  fabricating a catalogue. webOS answers
  `ssap://com.webos.applicationManager/listApps` through
  `WebosSessionPool::list_apps` (the URI already existed in `SsapUri` and was
  never used); Roku answers `/query/apps` through the facade now, rather than
  only from the caller. Entries with no id are skipped and a missing title stays
  `None`.
- **webOS input switching** (`ssap://tv/switchInput`): `WebosSessionPool::switch_input`
  plus an `InputSelect` implementation for `TvClient::Webos`, so the companion's
  Inputs sheet appears on an LG set. `active_input()` answers `None` — webOS has no
  reliable "which input is live" read. Ids are `HDMI_<n>` (1-based) and `tv` for the
  tuner; `Usb` and `Hdmi(0)` are refused rather than guessed at. Unverified on a
  real panel, so the ids are pinned by tests.
- **`VolumeControl::mute()`**: a read next to `set_mute`, with `Ok(None)` meaning
  "cannot tell". webOS answers `ssap://audio/getMute` (the URI already existed and
  was never used), `MockBackend` reports its own state, and MDC returns `None`
  rather than guessing.
- **An Android block no longer reads as "TV off" (companion gap A5)**: from Android
  17 (target SDK 37) a LAN socket needs the runtime `ACCESS_LOCAL_NETWORK`
  permission, and until the user grants it the platform refuses the call — `EPERM`
  from a blocked socket, which the NDK reports as
  `ANDROID_NETWORK_BLOCKED_REASON_LNP`. Every socket error used to be stringified
  into `TransportError::Unreachable`, which routes to *Retry*, so the UI offered
  *Wake* forever for a television that was awake and merely unreachable by this app.
  Core gains `TransportError::Blocked { reason }` with a `from_io()` classifier —
  the `EPERM` check compiles only on `target_os = "android"`, so desktop
  classification is unchanged — and `Remedy::RequestPermission`, because the UI must
  say "grant access" rather than "wake", with a `hint()` that names the grant. The
  facade's tizen, webOS, discovery and power mappings and MDC's socket arm route
  through the classifier, so a refused SSDP send is a blocked call instead of an
  empty discovery list. The healer maps the new remedy to `Quarantine`: the
  permission belongs to the platform, so retrying burns battery and waking wakes
  nothing.
- **Installed applications through the facade — companion gap G5**: `AppLaunch::installed_apps()`
  now has real arms, so the fleet grid's "what is on this television" column has a source.
  webOS enumerates through `ssap://com.webos.applicationManager/listApps` (the pool already
  parsed it; the facade never called it) and Roku through `/query/apps` — both answers map to
  `AppRef { id, name }`. The refusal names its own capability: `Capability::AppList` is new, and
  Tizen answers `Unsupported(AppList)` rather than `Unsupported(AppLaunch)`, because the remote
  channel starts an application it cannot enumerate and the two facts are independent. Advertised
  only where delivered, and the webOS reply shape is recorded in the playbook with its sources.
- **Roku foreground reads (`tv-rs-roku` + the facade — companion gap G10b, Roku half)**:
  `RokuClient::query_active_app()` fetches `GET /query/active-app` and
  `parse_active_app()` turns it into `RokuActiveApp` — the active app's id and name,
  plus any screensaver covering it. The facade's `foreground()` answers on Roku now and
  advertises `Capability::Foreground`, which is what an "on screen" card reads. The
  home screen reports `Roku` with no `id`, so that answers `None` rather than inventing
  an application; the three documented shapes (app, home screen, screensaver) each have
  a test. Tizen's half stays open: no foreground command for the consumer remote
  channel is verified in this workspace, and the candidate endpoint needs provenance
  before it becomes code.
- **webOS key input (`tv-rs-webos` + the facade — companion gap G10a)**: a press is
  now a `type:button` frame on the pointer socket `getPointerInputSocket` hands
  back. `PointerWire` / `PointerFactory` keep that connection testable without a
  television, and `WebosSessionPool::send_key` holds one pointer socket per TV,
  reuses it across presses and reopens it once on failure.
  `WEBOS_DEFAULT_PERMISSIONS` gains `CONTROL_INPUT_TEXT` and
  `CONTROL_MOUSE_AND_KEYBOARD` — the input service refuses the path without them,
  so a registration that omitted them paired fine and then had a dead D-pad. The
  facade maps `Key` to the channel's own button vocabulary (`webos_key`) and now
  advertises `Capability::Key` for webOS. **Unverified on a real panel**: the fake
  TV cannot answer SSAP, so the wire form is pinned by unit tests.
- **Wake, and the address it needs (companion gap A4)**: `TvClient` can wake a panel
  at last. Roku goes over ECP — `PowerControl::wake()` calls the `PowerOn` keypress the
  client already had, because a Roku answers while the screen is dark. Tizen and webOS
  drop their network stack when they sleep, so their only path is a broadcast magic
  packet: `TvClient::with_mac("aa:bb:...")` supplies the address and `wake()` then
  broadcasts it through `tv_rs_power::wake_on_lan`. `Capability::Wake` is advertised
  exactly where a wake can be delivered — always on Roku, on the other two once a MAC
  is known — never merely because the trait exists. `TvClient::learn_mac()` reads
  Roku's own `/query/device-info` (Wi-Fi address preferred over wired) and refuses on
  Samsung and webOS rather than guessing at an endpoint; `DeviceDescriptor::mac`
  reports whatever the handle knows, so a fleet can persist it. A malformed address is
  reported as `ProtocolError::Malformed` instead of being broadcast.
- **`RokuDeviceInfo::mac()`** — the Wi-Fi address when the box is wireless, the
  Ethernet one when it is wired.

### Added

- **`platform-verifier`: the operating system's root store for `tv-rs-transport`**
  (companion gap G7). `rustls-native-certs` reads a *system* store, which Android (a
  keystore) and iOS (its own roots) do not expose — so a mobile build with
  `TlsPolicy::Strict` had no roots to verify against. The new `platform-verifier`
  feature asks the operating system instead, through `rustls-platform-verifier` pinned
  at 0.7.1, the version whose API was verified against docs.rs rather than remembered.
  Additive and narrow: **only the `Strict` arm changes**. `Pinned` and
  `TrustOnFirstUse` install their own verifiers and never reach it, so the feature
  cannot weaken pinning — asserted by a test that builds all four policies with it
  enabled. The extension used is `BuilderVerifierExt::with_platform_verifier`, which
  takes the existing builder so the workspace's ring provider and protocol versions
  carry into the verifier; the no-arg `ConfigVerifierExt` would have discarded them.

### Changed

- **A silent SSDP probe is now an error, not an empty list** (`tv-rs-discovery` +
  the facade — companion gap G7). Discovery counted no datagrams: when `send_to`
  succeeded and nothing came back, the scan returned an empty `Vec`, which reads as
  "no displays on this network". Silence is a different statement and usually the
  wrong one — the multicast was dropped, which on Android means the app has not
  been granted local-network access. The scanner now counts the datagrams it
  receives and, when that count is zero, returns `DiscoveryError::SilentProbe`. The
  message names *both* explanations (nothing listening, or the local network
  unreachable) because neither is knowable from here, and the facade maps it to
  `TransportError::Blocked`, reusing A5's "grant access" remedy and `hint()` rather
  than inventing a third bucket. An empty list still means the network answered and
  no display was among the replies.

### Fixed

- **The `serde` feature compiles now.** It never did: `Serialize`/`Deserialize`
  were not imported in the files that used them, so
  `cargo check -p tv-rs-core --features serde` failed outright — the feature had
  never been built. Fixed with qualified derives
  (`derive(serde::Serialize, serde::Deserialize)`), `rename_all = "snake_case"` on
  the enums so `Capability::PowerState` → `"power_state"` and `Remedy::Retry` →
  `"retry"`, and derives on `PowerState`, `Confidence` and `AppRef`, which had
  none. `serde.rs`'s manual `Key`/`Input` impls passed `String` where
  `serialize_str` wants `&str` and were missing `SerializeMap`; `from_str` had an
  unworkable `'static` bound.
- **`ObservedField` no longer promises a `verified_at_ms`.** An `Instant` is
  monotonic and has no epoch, so an absolute timestamp is not derivable from it.
  The field is gone; the wire carries `age_ms`, `confidence` and `value` — which is
  what the companion's G2 was asking for.
- Every capability's serde output is now asserted equal to its `as_str()` label,
  so the derive and the label cannot drift. Two redundant `#[must_use]`s on
  functions returning `Result` are gone.

## [0.3.0] - 2026-10-24

### Added

- **MDC client conformance testing and publish readiness (`tv-rs-mdc`)**: added a
  conformance test in `crates/mdc/tests/tcp.rs` running `tv_rs_core::conformance::run`
  against `MdcDevice`, verified feature gating of LFD surfaces (`LampOrPanelHours`
  gated behind `supporting`, `ScreenCapture` never advertised), confirmed clean
  resolution in `xtask publish --dry-run`, and recorded the `UNVERIFIED` status
  row in `docs/hil-results.md`.

- **Public EDID parser and active-display EDID enumeration in `tv-rs-local`
  (SignageMaster handoff E3b)**: `tv_rs_local::parse_edid(&[u8]) -> Result<Edid,
  EdidError>` reads the base block per VESA E-EDID 1.4 §3 — PNP manufacturer id,
  product code, serial (numeric and descriptor), week/year, version, digital input,
  size in cm, the product-name descriptor and the preferred `DetailedTiming` (pixel
  clock, active/blank, image mm, `refresh_hz()`), extension count and a reported (not
  enforced) checksum. `display_edids() -> Vec<DisplayEdid>` returns the raw bytes and
  the parsed block for every active display: on Windows from
  `HKLM\SYSTEM\CurrentControlSet\Enum\DISPLAY\<id>\<instance>\Device Parameters\EDID`,
  addressed through the monitor's device interface path; on Linux from
  `/sys/class/drm`. The Windows probe uses that EDID when WMI has no record, and the
  Linux probe now fills `model` and `serial` from it. Tested against two real EDIDs
  from linuxhw/EDID (CC BY 4.0).

- **One error taxonomy at the facade (companion gap A2)**: `tv_rs::TvError` is now a
  re-export of `tv_rs_core::TvError` — the facade's parallel enum is deleted, not
  deprecated. Core gains `TvError::UnsupportedDevice { address, reason }` (a device
  answered and matched no signature) and `TvError::UnsupportedOperation { vendor,
  operation }` (the vendor has no such protocol message), both routing to
  `Remedy::Disable`, and a `hint()` method that returns a user-facing sentence where
  the error alone cannot. webOS `RegistrationFailed` now carries `presented_key`:
  a **declined prompt** maps to `Auth(PairingRequired)`, a **revoked stored key** to
  `Auth(TokenExpired)` — the app can finally offer the right button.

- **Optional serialization of the core device model (`tv_rs_core/serde`)**: the new
  `serde` feature (off by default) derives `Serialize`/`Deserialize` for the core
  data types — `Capability`, `Key`, `Input`, `AppRef`, `DeviceDescriptor`,
  `PowerState`, `TvError`, `Remedy`, `ObservedState` — with the wire contract that
  goes to the companion: `Key::Digit(n)` → `"digit_n"`, `ObservedField` →
  `{"verified_at_ms": u64, "age_ms": u64, "confidence": "high"|"inferred"|"unknown",
  "value": T}`. No extra dependency in the default build, and the companion can
  hand the core types straight to its `types.ts` instead of maintaining a DTO fork.

- **`tv_rs::map` — the one home of vendor-error mapping**: three exhaustive
  per-vendor functions (`tizen`, `webos`, `roku`) plus `discovery` and `power`.
  No wildcard arms — a new vendor error variant is a compile error until someone
  assigns its remedy, and a table-driven test walks every variant. Consumers of the
  protocol crates directly (the CLI) route through the same functions.

- **`DiscoveredTv.to_client` / facade errors for unknown devices** answer with
  `UnsupportedDevice`, which unlike the old `UnsupportedVendor` carries the address
  a fleet dashboard can act on.

- **webOS session pool (`tv-rs-webos`)**: one registered SSAP session per TV,
  reused across commands instead of paying a dial + `register` handshake per call
  (which without a stored key meant an on-screen prompt per command). Idle limit
  60 s, a dead session reopens exactly once before the failure surfaces, and
  the pool serialises requests per TV to match the protocol.

- **webOS pointer channel**: `SsapUri::NetworkInputGetPointerSocket` plus
  `PointerSocket` — a second WebSocket computed from the per-session
  `socketPath` the TV returns, carrying byte-exact plain-text pointer frames
  (`type:move` / `type:button` / `type:click` / `type:scroll`, always with the
  `down:` flag, blank-line terminated). Fire-and-forget as the TV acknowledges
  nothing; `close()` gives the last frame the 120 ms grace the TV needs before
  dropping near-close writes. Provenance-traced to four independent
  implementations and one measured writeup (webOS 23). New enum variant; per
  the 0.x policy that is a minor bump.

- **The facade holds the connection pools (companion gap A1)**: `TvClient`'s Tizen
  and webOS variants now carry the pool next to the protocol client, so a key press,
  a volume change or a foreground read reuses one connection per TV instead of paying
  TCP + TLS + handshake on every call — the cost the facade was paying while
  `TizenConnectionPool` sat unused. Each pool is shared behind an `Arc`, so cloning a
  handle clones the *connection*, not the right to open another one; `TvClient::close_all()`
  releases them for mobile backgrounding; and the factories are erased
  (`DynConnectionPool` / `DynSessionPool`) so a caller can inject its own. webOS still
  refuses key presses: SSAP has no key endpoint, and the pointer socket that does is a
  separate item. Proven without hardware — `cargo test -p tv-rs --features testkit`
  presses twenty keys through the facade and asserts exactly one channel was opened.

- **`testkit` features in `tv-rs-tizen`, `tv-rs-webos` and `tv-rs`**: counting channel
  and session factories, so a downstream crate can prove connection reuse without a
  television. Off by default; a released build carries no test code.

- **`tv-rs-app` — a TV app producer**: package a supplied web/wasm payload as an
  installable TV app. Tizen `config.xml` and webOS `appinfo.json` generation, the
  bundle layout, a stored-entry ZIP writer (a `.wgt` **is** a ZIP, so the archive is
  written here — no compression dependency, and byte-identical for the same input),
  and the generated `tv-bridge.js` that maps the D-pad onto a payload's
  `key_event(name, value)`. Vendor back/exit numbers are supplied by the deployment
  rather than guessed, and a `.ipk` is never fabricated: the crate returns the webOS
  app directory and the `ares-package` command. New crate, so it publishes with the
  rest of the workspace.

- **MDC over RS-232** (`tv-rs-mdc`, feature `serial`): the same frames, checksum and
  reply parsing as the TCP path, on a serial line — a wired video wall has no
  discovery, no DHCP and no TLS to argue about. `MdcClient::serial("/dev/ttyUSB0",
`9600`) opens it; the request loop is carrier-agnostic and a test drives it over an
in-memory channel to prove the codec is shared rather than copied. `serial`is off
by default, so a cloud agent that only talks to networked panels compiles no serial
stack, and an absent device is a typed`MdcError::Io` rather than a hang.

- **Commercial LFD capability surfaces** (`tv-rs-core` + `tv-rs-mdc`): a 40-inch
  wall panel is not a TV remote, and the vocabulary therefor is now in the device
  model. `Capability` grows `PanelLock`, `Orientation` and `NoSignalStatus` as
  always-on surfaces, plus `LampOrPanelHours` and `ScreenCapture` behind the new
  **`supporting`** feature. Core ships `PanelLockControl`, `OrientationControl`,
  `PanelHealthProvider` (`PanelHealth`: signal present, temperature, lamp fault)
  and — behind `supporting` — `PanelHoursProvider` (`PanelHours`) and
  `ScreenCapturer` (`ScreenFrame`). The mock backend advertises and answers all
  of them from settable state, and the conformance suite covers truth table and
  `Unsupported` routing for all five.

- **The LFD subset on MDC**: `tv-rs-mdc` advertises and implements exactly what
  its protocol speaks — panel lock (`0x5F`), content orientation
  (`0xC8`/`0x82`; `MdcOrientation` maps onto `tv_rs_core::Orientation`),
  panel health (`0x0D`: lamp fault, input-source error, panel temperature, fan)
  and panel on-time (`0x83`, ten-minute units converted to hours, behind
  `supporting`). Every surface has wire-frame tests, including the `0x02`
  invalid-selection case of `0x0D`. That `0x0D` reply also carries panel temperature
  and input-source state, which is why the health surface exists: MDC does report
  those two, and an earlier draft of this file said it did not.

- **Tizen states its absence**: the consumer remote channel (8001/8002) exposes
  none of the LFD subset, so `TvClient` advertises none of it — documented in the
  facade's capability table and the state matrix rather than faked behind a
  adapter that cannot work.

- **Graduated remediation ladder** (`tv-rs-agent::remedy`): the agent no longer stops
  at a telemetry verdict. A panel that is hot but not critical is **dimmed**
  (`Capability::Brightness`) and deferred; a panel whose temperature or link is
  critical is **rebooted once** and quarantined either way, because a wedged panel
  often answers after a restart; a live input whose player stopped can be
  **restarted** (`Capability::Key`). Every rung is a pure function of (snapshot,
  verdict, capabilities) and a rung whose capability is missing is skipped, never
  faked — a panel that cannot dim is not "dimmed" by a log line. The audit entry names
  the step that ran (`telemetry_dim`, `telemetry_restart`, `telemetry_reboot`) or the
  verdict alone (`telemetry_wait`, `telemetry_quarantine`) when there was nothing to
  do.

- **`tv-rs-mdc`**: Samsung Multiple Display Control over TCP 1515 — the protocol
  commercial signage panels actually expose (RJ45; the same frames travel over RS-232).
  Frame encoding with checksum validation, display-ID addressing including the
  broadcast address, one shared socket for every panel in a video wall, and power,
  input, volume, mute, brightness and aggregated status. A `NAK` is returned with its
  command and error code and the socket stays open, because the wire is still in sync;
  a timeout or a malformed reply drops the socket so the next command reconnects.

  This is a different protocol from the consumer Tizen channel in `tv-rs-tizen`: no
  WebSocket, no TLS, no pairing — which is exactly why it exists. Its `DeviceHandle`
  adapter covers power, input, volume, **brightness**, **reboot**, a control-channel
  **probe** and the LFD surfaces above. It does not advertise aggregate `Telemetry`:
  MDC reports panel temperature and input-source state (exposed as `PanelHealth`) but
  no signal-strength history and no uptime, so a `TelemetryProvider` would have to
  invent two of its four fields.

- **Telemetry** (`tv-rs-core`): temperature, signal strength, signal quality and
  uptime, each an `ObservedField` — a panel that stops answering is not cool and
  well-fed, it is unknown. Plus the `TelemetryProvider` trait and the
  `Brightness`/`Reboot`/`Telemetry` capabilities, with `BrightnessControl`,
  `Rebootable` and `Probe` to drive them.

- **Telemetry policy** (`tv-rs-agent`): `assess_telemetry()` turns a snapshot and
  thresholds into `Healthy` / `Wait` / `Quarantine` as a pure function — no clock, no
  counter, no "third reading" state machine. The runner skips a hot panel instead of
  commanding it, quarantines a critical one, and a failed telemetry read never blocks
  reconciliation.

- **`DeviceOs::SamsungMdc`**, because an MDC panel is not "unknown": no pairing, no
  WebSocket, a display id instead of a remote.

### Changed

- Facade inherent methods (`open_url`, `power_off`, `power_state`,
  `power_observation`, `launch_app`) and all seven `DeviceHandle` trait impls return
  core errors directly; vendor-bound calls are mapped at the call site with the
  capability the call site needed `to_core` deleted.

- **`Remedy::Quarantine`** joins the taxonomy, and `StateError::Quarantined` maps to
  it: the capability still exists, the device's condition forbids using it — which is
  the difference from `Disable`.

### Removed

- **The specification documents** (`docs/spec/00-device-model.md`,
  `docs/spec/01-reconciliation.md`). They described a model that shipped; the design
  now lives in rustdoc, `docs/architecture.md` and this file, and `docs/06-plan.md` is
  the only plan. The adoption contract moved to `docs/07-adoption.md`.

## [0.2.0] - 2026-10-20

Thirteen crates. Two are new (`tv-rs-local`, `tv-rs-dlna`), `tv-rs-tizen` gains the U1
remote service, and the facade learns what confidence and quirks mean.

`TizenError` gained a variant (`InvalidKey`), which the versioning policy in
`CONTRIBUTING.md` classes as breaking at `0.x` — hence `0.2.0` rather than `0.1.1`.

### Added

- **`tv-rs-local`** (new crate): probes the host's own displays — EDID identity,
  connector, mode, physical size and a TV-likelihood score — with no television on the
  network. Windows uses WMI + GDI, Linux DRM/EDID; a platform failure is a typed error,
  never a panic.
- **`tv-rs-dlna`** (new crate): DLNA renderer transport — SSDP discovery of
  MediaRenderers, AVTransport SOAP (`SetAVTransportURI`, `Play`, `Pause`, `Stop`,
  `Seek`), GENA subscribe/renew/unsubscribe with `LastChange` parsing and a renewal
  registry, SSDP advertisement for a server the caller hosts, and DIDL-Lite metadata.
  It is transport only: no media library, no server, no ContentDirectory.
- **`TizenRemote`**: the stateful Samsung service for fleet callers. It owns the token
  store, the connection pool and per-host TLS trust, and exposes pairing, raw/typed
  keys, app launch, `disconnect`, `forget_paired` and `shutdown`.
- **Pairing flow** (`TizenClient::pair`): 30-second on-screen window, token and TV name
  from the handshake, accepting both `name` and `deviceName` firmware spellings.
- **Raw key codes** (`send_key_code`): any validated Samsung `KEY_*` code, so
  model-specific keys no longer need a closed enum; an invalid code is a typed error.
- **Channel launching**: `ed.apps.launch` with `NativeLaunch`/`DeepLink`, the legacy
  `ms.channel.launch` event, and a channel → DIAL → legacy fallback that starts with
  DIAL when the TV is not paired.
- **Quirks are consumed, not just loaded.** `tv-rs-core` decodes a row's `action` into
  a typed `QuirkAction`, and with the facade's `quirks` feature a matching row pins the
  port at client construction: a `QN90*` Tizen panel is built on 8002 only, a webOS 23
  panel on 3001 only, so the dead generation is never probed. A test asserts every
  action shipped in `quirks/default.toml` decodes to known vocabulary.
- **`power_observation()`**: the facade returns the power state _and_ its confidence.
  Roku is `High` (it reports the mode); Tizen and webOS are `Inferred`, because an
  answering control port does not prove a picture is on. `power_state()` is a
  projection of it, so the two cannot disagree. `tv-rs-cli status` prints `CONFIDENCE`.
- **`INTEGRATION.md`**: the adoption guide — discovery, pin-on-first-use, quirks, a
  fleet, media, CEC, and the pre-ship checklist.
- **`crates/tizen/README.md`** marks Tizen experimental and says exactly why: mocks
  prove the parsers, not that Samsung sends those frames.
- **24-hour fleet simulation** (`twenty_four_simulated_hours_with_three_blips`): 50
  devices, three panels that drop into standby on their own, two that never pair, one
  whose power is never observable. Every healthy device converges every hour, each blip
  costs exactly one wake, and no device exceeds one action per hour. Backed by
  `MockBackend::force_power`, so a test can make a television change state without a
  command.
- **Versioning policy** in `CONTRIBUTING.md`: what counts as breaking at `0.x`, and the
  90-day freeze on removing error variants.

### Changed

- The connection pool drains the remote channel's read half in the background, so a
  silent TV cannot fill its receive buffer and make the next key press look dead.
- `#![forbid(unsafe_code)]` covers every crate, with one documented exception:
  `tv-rs-local`'s Windows FFI module, which is the only code in the workspace that calls
  platform APIs the safe crates do not expose.
- `tv-rs` re-exports `tv-rs-transport`, so `TlsPolicy` and `cert_sink` are reachable
  from the facade that already needs them.

### Fixed

- **Two RUSTSEC advisories in `quick-xml`** (quadratic duplicate-attribute scan,
  unbounded namespace allocation — both denial of service). The DLNA crate now pins
  `0.41`, the first patched release, and uses the split decode/unescape API.
- `ms.error` during the Tizen handshake maps to `Unauthorized` instead of an untyped
  protocol error.

## [0.1.0] - 2026-10-20

First published release: eleven crates on crates.io, all at `0.1.0`.

### Added

- **`tv-rs-agent`**: the reconciliation layer. A `DesiredStateProvider` seam
  (`StaticFileProvider` reads a JSON map), a pure `decide()` function that turns
  desired + believed + capable into one action, a `HealerLadder` that escalates
  Wake → Reconnect → Quarantine (and never retries a missing credential), and an
  append-only JSONL audit log. A 50-device mock fleet converges, quarantines the
  pathological devices, and never sends a power toggle it cannot verify.
- **Two more desired-state providers.** `HttpPollProvider` (feature `http`) polls a
  JSON manifest with a 5 s timeout and separates "transport failed" from "manifest
  is wrong"; `MqttSubscribeProvider` (feature `mqtt`) subscribes to
  `<prefix>/+/desired`, keeps the latest payload per device and treats a malformed
  message as one bad publisher rather than a broken subscription. Both are off by
  default so the agent's dependency graph stays small.
- **Agent run loop.** `Reconciler::tick()` makes one pass over the fleet;
  `run(stop)` repeats it until told to stop. Per device: ask the provider, read the
  power state, decide, act, audit, and escalate through the healer. A quarantined
  device is skipped on later passes, a failed audit write never stops convergence,
  and one device failing does not abort the pass for the rest.
- **Conformance suite** (`tv-rs-core`, feature `conformance`): every `DeviceHandle`
  must pass capability/accessor agreement, unsupported-routing and identity checks.
  The facade's three clients pass it; the mock does too, which is how the suite
  itself is validated.
- **Quirks table** (`tv-rs-core`, feature `quirks`, `quirks/default.toml`): Samsung
  8001/8002/55000 generations, LG pre/post-2023 ports and the renamed wake-on-LAN
  setting, Roku OS 14.1's "Control by mobile apps" gate and OS 15's
  `supports-tv-power-control` flags. First match wins, and file order is the
  precedence — pinned by a test.
- **Transport negotiation.** Tizen and webOS now try both port generations
  (`wss://:8002` → `ws://:8001`, `wss://:3001` → `ws://:3000`), remember the one
  that answered per client, and only fall back when the connection was _refused_ —
  a certificate mismatch is an answer, not a reason to try plaintext. Connection
  attempts are bounded at 5 s and the Tizen handshake at 10 s, so a panel that
  accepts the socket and goes quiet no longer hangs a caller.
- **`TvClient::open_url()`** — the signage primitive: webOS puts the URL in its
  built-in browser (`ssap://system.launcher/open`). Tizen and Roku return
  `UnsupportedOperation` rather than launching an empty browser.
- **`TvClient::from_discovered_with(tv, TlsPolicy)`** — the function that makes
  certificate pinning reachable from the facade; `from_discovered` keeps `Strict`.
- **`tv-rs-core`: the device model.** Capability traits (`PowerControl`,
  `KeyInput`, `AppLaunch`, `InputSelect`, `VolumeControl`), the erased
  `DeviceHandle`, `CommandId` with its idempotency contract, confidence-tagged
  `ObservedField`/`ObservedState`, the six-variant routing `TvError`, and a
  scriptable mock backend (`mock-backend`) that reproduces pre-send failures,
  lost acks, malformed replies, mid-session auth expiry and mid-command power-off.
  The crate has no tokio in its dependency graph in any configuration; CI enforces
  that.
- **The facade speaks the device model.** `TvClient` implements `DeviceHandle`
  and the five capability traits, with per-vendor capability sets that match what
  each backend actually does, and backend errors converted into the routing
  taxonomy (`Unsupported`, `Auth(PairingRequired)`, `Transport(Timeout)`, …).
- **Certificate pinning for self-signed TV certificates.** `tv-rs-transport`
  gains `CertFingerprint` (SHA-256, hex round-trip), `TlsPolicy::Pinned` and
  `TlsPolicy::TrustOnFirstUse` with a `CertSink`. The default policy stays
  `Strict`; a pinned channel verifies the exact certificate a TV presented,
  which is what makes Samsung 8002 / LG 3001 usable without
  `TlsPolicy::AcceptInvalid`. Rustls only — a `native-tls` build returns
  `TlsError::PinningUnavailable`.

- **Power state across the facade.** `TvClient::power_state()` answers "is this TV
  on?" without reaching for a vendor crate: Roku is queried through
  `/query/device-info` (`RokuPowerMode`, with `Headless`/`Ready`/`DisplayOff`
  reported as `Standby`, not `Off`), Tizen and webOS fall back to the shared TCP
  probe against their control port.

- **Facade (`tv-rs`)** — `discover()` / `discover_with()`, a documented
  `identify_vendor()` heuristic over SSDP and UPnP fields, `DiscoveredTv`, and
  a `TvClient` that dispatches power and app-launch calls across Samsung, LG
  and Roku with typed `as_tizen` / `as_webos` / `as_roku` accessors.
- **`tv-rs-discovery`** — SSDP `M-SEARCH` over UDP 1900 with interface/bind
  overrides, case-insensitive response parsing with USN/LOCATION dedup, and
  UPnP device-descriptor parsing.
- **`tv-rs-power`** — Wake-on-LAN magic packets (UDP 9) and a TCP power-state
  probe distinguishing On / Standby / Off / Unknown.
- **`tv-rs-tizen`** — remote WebSocket channel on 8001/8002 with the
  `ms.channel.connect` handshake and token capture, remote-key frames, and the
  DIAL launcher on 8080.
- **`tv-rs-webos`** — SSAP JSON-RPC over WebSocket on 3000/3001 with pairing,
  `client-key` persistence, and launch/volume/mute/power/foreground commands.
- **`tv-rs-roku`** — unauthenticated ECP REST on 8060: device-info, apps,
  keypress and launch, with pure XML parsers.
- **`tv-rs-transport`** — shared TLS policy, native-tls connector and budgeted
  WebSocket frame reader.
- **`tv-rs-media`** — HTTP `206 Partial Content` serving (open-ended, explicit
  and suffix ranges, `416` handling) plus HLS `VOD` manifest generation.
- **`tv-rs-cli`** — `discover`, `wake`, `power-off`, `key` and `launch`
  subcommands (`publish = false`).
- **Hardware-in-the-loop harness** behind the `hil` feature, skipped unless
  `TV_RS_HIL_*` host variables are set.
- MIT license, CI (rustfmt, clippy, test on Linux+Windows, rustdoc) and
  `cargo xtask publish`, which releases the eleven crates in dependency order.

### Changed

- **Phase 6 hardening.** `rust-version` corrected from `1.75` to `1.89` — the old
  value was never buildable (`uuid` and the ICU crates behind `idna` already require
  1.88–1.89), and CI now checks the claim in an `msrv` job. Added a `cargo-deny` job
  with a deliberate advisory/licence/bans policy, an explicit-features rustdoc job
  (`--all-features` is impossible by design: both TLS backends at once is a compile
  error), and per-crate README/`deny.toml`. Removed a stray `du.exe.stackdump` and
  the per-crate `LICENSE` copies, which were eleven chances to drift from the one at
  the root.
- **Workspace reorganization.** All packages are flattened under `crates/` (one
  home per package), the facade moved from the repository root to
  `crates/tv-rs`, the CLI was promoted from `utils/cli` to `crates/cli`, and the
  root manifest is now a virtual workspace. Release tooling was ported from a
  shell script to `cargo xtask publish`.
- `missing_docs` is enforced once, in `[workspace.lints.rust]`, instead of in
  every crate root.

### Removed

- `roku`'s no-op `RokuAuthProvider` marker trait: Roku ECP is unauthenticated, and
  a marker that always reports "no credentials" was noise.

### Fixed

- **Tizen frame budget is per read.** `TizenSession` decremented one counter for the
  life of the session, so after a dozen reads every command reported a timeout. Each
  `recv_json` now takes a fresh budget, and the counter is no longer stored.
- **Tizen DIAL reuses one HTTP client.** `launch_app_via_dial` and
  `dial_device_description` built a `reqwest::Client` per call, discarding the
  connection pool and re-resolving DNS every time.
- **`Debug` no longer prints credentials.** `TizenClient`'s pairing token and
  `WebosClient`'s `client-key` are redacted: both are bearer credentials, and a log
  line containing one hands over the screen.
- The facade's `keywords` contained path-like entries with slashes, which
  crates.io rejects.
- The facade `description` advertised mDNS discovery, which is not implemented;
  mDNS is now described as planned.

### Known limitations

- Vendor authentication that depends on undocumented behaviour (self-signed TLS
  pinning, enterprise token derivation, encrypted framing) is unimplemented. It
  surfaces as typed `Unsupported` errors rather than panics or a disabled
  certificate check.
- mDNS/Bonjour discovery and Android TV ADB are not implemented.
- No crate has been verified against physical hardware in CI; use the `hil`
  harness.

