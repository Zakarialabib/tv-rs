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
order — `tv-rs-core` first, `tv-rs-mdc` among the leaves before the crates that name
it. The order is data, not prose: `xtask/src/main.rs::ORDER` is the single home,
and [ADR-001](adr/001-crate-publication.md) records why it is shaped that way.

**Exit:** fourteen crates at `0.3.0`, a fresh `cargo add tv-rs` consumer builds against
`power_observation()` and `assess_telemetry`, docs.rs serves the new items, and the
CHANGELOG and README agree with the registry.

## 2. `tv-rs-sony` — Bravia REST + IRCC

Sony's consumer and pro panels expose a documented JSON-REST API plus IRCC-IP for key
presses, authenticated with a pre-shared key. It is the smallest remaining vendor
integration: power, input, volume, app launch, and key codes.

**Exit:** passes the conformance suite; capability set is honest (no absolute volume if
the API only takes steps); pairing failure maps to `Auth` with `Remedy::Reprovision`.

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
answers SSDP. Until then every protocol claim in this repository is
documented-and-mocked.

**Exit:** at least one row per supported vendor (Tizen, webOS, Roku, MDC), each stating
what was commanded and what came back — or an explicit "not verified" line, which is
also a result.

## 6. Local media caching and multi-screen sync

`tv-rs-media` serves a remote panel's stream. The same node should cache to local disk
and drive several panels from one producer without pulling the source through the
network twice: a player-level cache plus a sync coordinator on `tv-rs-media`.

**Exit:** a local cache hit is measurable and reportable; two-panel sync shows no double
network transfer for a shared source; no regression in the existing `Media` traits.

## 7. Release `0.4.0`

Every open item above, shipped in the order of this page: `tv-rs-core`, protocol
crates, `tv-rs-mdc`, then the agent, media and facade. Under the policy in
`CONTRIBUTING.md` the additions are a **minor** bump.

**Exit:** all fourteen crates published at the same version; `docs/hil-results.md` has
at least one real row; the vendor capability table
([`03-matrix.md` §Capability truth](03-matrix.md#capability-truth)) states each
vendor's absence rather than implying a capability it lacks.
