# Plan

**This is the only plan.** The specification documents that used to sit beside it
(`spec/00-device-model.md`, `spec/01-reconciliation.md`) are deleted: they described a
model that shipped, and a design that ships is documented by its code, its rustdoc and
[`architecture.md`](architecture.md), not by a proposal that keeps a phase table
forever. The P0–P9 proposals are archived in [`05-roadmap.md`](05-roadmap.md).

Rule for this page: an item exists here only while it is **not** done. When it lands,
it moves to the CHANGELOG and the matrix, and its section is deleted. A plan that
accumulates ticks is a second changelog, and a worse one.

> **How this page was updated (2026-10-22):** the fronts the 2026-10-21 review asked
> for have all closed. The graduated agent ladder landed
> (`crates/agent/src/remedy.rs`), the commercial LFD capability surfaces landed in
> core and `tv-rs-mdc`, and MDC now speaks RS-232 as well as TCP behind the `serial`
> feature; their sections are gone, as this page's rule requires. The one piece of the
> LFD work that no protocol can carry — `ScreenCapture` — is recorded in _Explicitly
> rejected_ with the sources that were checked, so it does not come back as a
> suggestion. Every remaining claim below is a _to-do_, not a _state_.

---

## Where things stand

- **`0.2.0` is published** — thirteen crates on crates.io, verified by a fresh
  `cargo add tv-rs` consumer.
- **`0.3.0` is tagged and unpublished** — `v0.3.0` exists, `main` carries the bump,
  and the crates.io upload is waiting on a token with the `publish-update` scope.
  Until it lands, the registry still serves `0.2.0`.
- **`tv-rs-mdc` is merged and unreleased** — Samsung MDC over TCP 1515, the protocol
  commercial signage panels expose. Its manifest is `0.3.0`; that version does not
  yet exist for it on the registry.
- **Telemetry is merged and unreleased** — `Telemetry`/`TelemetryProvider` in core,
  `Brightness`/`Reboot`/`Telemetry` capabilities, `Remedy::Quarantine`, the agent's
  `assess_telemetry` policy, and `DeviceOs::SamsungMdc`.
- **The agent has a graduated remediation ladder** — a hot panel is dimmed, a
  critical one is rebooted once before it is quarantined, a stalled player can be
  restarted; every rung is skipped rather than faked when the device cannot take it.
- **`tv-rs-mdc` speaks two carriers** — TCP 1515 by default and RS-232 behind the
  `serial` feature, over one frame codec: a wired wall is the same protocol on a
  different wire, and the request loop is carrier-agnostic (proved by a test that
  drives it over an in-memory channel).
- **LFD capability surfaces have landed** — `PanelLock`, `Orientation`,
  `NoSignalStatus` as always-on capabilities, `LampOrPanelHours` and `ScreenCapture`
  behind the `supporting` feature; the conformance suite covers all five; `tv-rs-mdc`
  implements what its protocol actually speaks (panel lock `0x5F`, content
  orientation `0xC8`/`0x82`, panel health `0x0D`, panel on-time `0x83`). Tizen,
  webOS and Roku expose none of the subset and say so per protocol.
- **Hardware is still unverified.** `docs/hil-results.md` is empty and that is the one
  honest gap in the v0.2 definition of done.
- **An external review of the workspace** (attached, 2026-10-21) confirmed the
  architecture's strengths and requested four fronts; all four have now closed —
  three by landing, one by a documented negative.

---

## 1. Release `0.3.0`

Three breaking additions have accumulated since `0.2.0`: `Remedy::Quarantine`,
`DeviceOs::SamsungMdc`, and the three new capabilities (`Brightness`, `Reboot`,
`Telemetry` as device capabilities). Under the policy in `CONTRIBUTING.md` that is a
**minor** bump, not a patch.

Order matters: `tv-rs-core` first (everything else resolves it from the registry),
then the protocol crates, `tv-rs-mdc` last among the leaves — it is new, so its first
upload is `0.3.0`. `cargo xtask publish --version 0.3.0` rewrites every pin and
publishes in that order:

`tv-rs-core -> tv-rs-transport -> tv-rs-discovery -> tv-rs-power -> tv-rs-local ->
tv-rs-dlna -> tv-rs-roku -> tv-rs-mdc -> tv-rs-tizen -> tv-rs-webos -> tv-rs ->
tv-rs-media -> tv-rs-agent -> tv-rs-cec`

**Why it blocks everything else:** `cargo package -p tv-rs-mdc` fails today because
the packaged tarball resolves `tv-rs-core 0.2.0` from crates.io, which has none of the
new APIs (`E0407`/`E0432`). The crate is fine; the registry is behind. The version
bump, the lockfile and the ordered publish are all in place — the first upload is
refused with `403 Forbidden` because the available token is `publish-new` only, and
updating an existing crate needs `publish-update`.

**Exit:** fourteen crates at `0.3.0`, a fresh `cargo add tv-rs` consumer builds
against `power_observation()` and `assess_telemetry`, docs.rs serves the new items,
CHANGELOG and README agree with the registry, and `docs/hil-results.md` carries an
explicit _not verified_ line per vendor (the honest gap, not a guess).

## 2. `tv-rs-sony` — Bravia REST + IRCC

Sony's consumer and pro panels expose a documented JSON-REST API plus IRCC-IP for key
presses, authenticated with a pre-shared key. It is the smallest remaining vendor
integration: power, input, volume, app launch, and key codes.

**Exit:** passes the conformance suite; capability set is honest (no absolute volume
if the API only takes steps); pairing failure maps to `Auth` with `Remedy::Reprovision`.

## 3. Native CEC — only after a measurement

`tv-rs-cec` shells out to `cec-client`. That is a deliberate trade (no `unsafe`, no C
toolchain), and it is fine until someone shows the subprocess latency hurts. If that
measurement exists, add a `cec-native` feature that talks to `/dev/cec` with `unsafe`
confined to one module; keep the subprocess adapter as the default.

**Exit:** a number, not an opinion — p50/p95 for a standby command through each path,
and no new botched error paths.

## 4. DDC/CI in `tv-rs-local`

Brightness and input for the display the machine is _attached to_, over `/dev/i2c`
(`VCP 0x10`, `VCP 0x60`). It belongs in `tv-rs-local` because that crate already owns
"this host's panel", and it must never be confused with controlling a remote panel.

**Exit:** Linux-only, feature-gated, reads VCP codes on a real monitor, and returns a
typed error where i2c is unavailable.

## 5. Hardware verification — the only unmet gate

`cargo xtask hw-test --write-doc` fills `docs/hil-results.md` the moment a television
answers SSDP. Until then every protocol claim in this repository is documented-and-mocked.

**Exit:** at least one row per supported vendor (Tizen, webOS, Roku, MDC), each stating
what was commanded and what came back — or an explicit "not verified" line, which is
also a result.

## 6. Local media caching and multi-screen sync

`tv-rs-media` serves a remote panel's stream; the review asked for the same node to
cache to local disk and to sync several panels from one producer without buffering
each through the network twice. Worked out as a player-level cache + a sync
coordinator on `tv-rs-media`.

**Exit:** a local cache hit is measurable and reportable; two-panel sync shows no
double network transfer for a shared source; no regression in the existing `Media`
traits.

## 7. Release `0.4.0`

The LFD capabilities, the agent's remediation ladder and every open item above. Under
the policy in `CONTRIBUTING.md` that is a **minor** bump, shipped in the order of the
plan _before_ it: `tv-rs-core`, protocol crates, `tv-rs-mdc`, then the agent, media and
facade.

**Exit:** the new capabilities pass conformance on MDC (and the other vendors state
their absence in the capability table); the agent ladder is live end to end;
`docs/hil-results.md` has at least one real row; all fourteen crates published at the
same version.

---

## Explicitly rejected

Recorded so they do not come back as "suggestions" a year from now:

| Idea                                                      | Why not                                                                                                                          |
| --------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| `PowerState::PoweringOn/PoweringOff` as observable states | no consumer panel reports them; a TCP probe cannot tell "warming up" from "off", so the enum would turn an inference into a fact |
| `trait PowerControl: TvDevice` with `&mut self`           | kills `Arc<dyn DeviceHandle>` and forces one lock per call; connection lifecycle belongs inside the backend                      |
| Stringly-typed errors (`AuthFailed(String)`)              | removes `Remedy`, which is the only reason a caller can branch on what to _do_                                                   |
| `create_device(...) -> Box<dyn TvDevice>` with `todo!()`  | unimplemented factory in a public API; the facade already dispatches on vendor                                                   |
| `type Percentage = u8` as a "bounded" type                | an alias enforces nothing; a brightness control needs a validating newtype                                                       |
| A CMS/scheduler inside this workspace                     | `NON_GOALS.md`; the agent converges a device, it does not decide what plays                                                      |
| A `ScreenCapture` backend from a vendor control protocol  | checked 2026-10-22 and none carries a capture frame: Samsung MDC has no capture command, Roku's documented ECP command list has none, LG states no screen-capture API is provided, and Samsung's remote channel plus `api/v2` expose none. The capability stays in the vocabulary (behind `supporting`) so a future protocol can implement it; no backend advertises it today, and inventing a frame to fill the gap is exactly what this repository refuses to do |

## Definition of done for `0.3.0`

1. `Remedy::Quarantine`, `DeviceOs::SamsungMdc`, and the three new capabilities land in
   core; the agent's `assess_telemetry` policy ships.
2. `tv-rs-mdc` published (RS-232 is already behind the `serial` feature).
3. `tv-rs-sony` merged and conformance-passing.
4. Every crate published at the same version; no new `unsafe` outside the two
   documented modules (`tv-rs-local`'s Windows FFI, and `tv-rs-cec` if native lands).
5. `docs/hil-results.md` carries an explicit "not verified" line per vendor.

## Definition of done for `0.4.0`

1. The new capabilities conform on MDC; every other vendor's absence is stated in the
   capability table rather than implied.
2. The agent's graded ladder is live end to end (dim → restart/input → reboot/PDU).
3. `docs/hil-results.md` has at least one real row, or an explicit per-vendor
   "not verified" statement.
4. Local media caching and multi-screen sync show no double transfer.
5. All fourteen crates published at `0.4.0`; CHANGELOG and README agree with the
   registry.
