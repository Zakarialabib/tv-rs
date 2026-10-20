# Plan

**This is the live plan.** It replaces the proposal list in
[`05-roadmap.md`](05-roadmap.md), which is now an archive of closed proposals, and it
supersedes the phase tables in [`spec/00-device-model.md`](spec/00-device-model.md),
which are history.

Rule for this page: an item exists here only while it is **not** done. When it lands,
it moves to the CHANGELOG and the matrix, and its section is deleted. A plan that
accumulates ticks is a second changelog, and a worse one.

---

## Where things stand

- **`0.2.0` is published** — thirteen crates on crates.io, verified by a fresh
  `cargo add tv-rs` consumer.
- **`tv-rs-mdc` is merged and unreleased** — Samsung MDC over TCP 1515, the protocol
  commercial signage panels expose. Its manifest is `0.2.0`; that version does not
  exist for it on the registry, so it can be published as-is.
- **The telemetry lane is in flight** (see below) — uncommitted work in `tv-rs-core`,
  `tv-rs-agent` and `tv-rs-mdc`.
- **Hardware is still unverified.** `docs/hil-results.md` is empty and that is the one
  honest gap in the v0.2 definition of done.

---

## 1. Land the telemetry lane — *in flight*

**What exists in the working tree:** a `Telemetry` snapshot where every metric is an
`ObservedField` (`temperature_celsius`, `signal_strength`, `signal_quality`, `uptime`),
a `TelemetryProvider` trait, three new capabilities (`Brightness`, `Reboot`,
`Telemetry`), the matching traits (`BrightnessControl`, `Rebootable`, `Probe`), a
`TelemetryPolicy` on the agent's `ReconcilePolicy`, and the MDC backend implementing
the read.

**Why it matters:** power/input/volume say what a screen is *doing*; telemetry says
whether it can keep doing it. A fleet that cannot tell "the app crashed" from "the
panel is at 78 °C" sends a technician to the wrong place.

**Exit criteria**

- `cargo test --workspace` green with the mock backend exercising every telemetry path;
- the conformance suite accepts a backend that reports telemetry and one that does not;
- one agent rule uses it (over-temperature or signal collapse → `Remedy::Wait` /
  `Quarantine`), with the decision still pure and testable without I/O;
- `docs/architecture.md` module inventory lists `telemetry.rs` for core and agent;
- CHANGELOG entry under the next version.

**Risk:** scope creep into a metrics pipeline. The seam is `TelemetryProvider`; the
crate reports, the caller decides what to store.

## 2. Publish `tv-rs-mdc`

No version bump required: `tv-rs-mdc 0.2.0` is a new crate. `cargo xtask publish`
already lists it (fourteenth, after `tv-rs-roku`), and the token now has
`publish-update`, so later bumps work too.

**Exit:** `cargo add tv-rs-mdc` resolves; docs.rs serves it.

## 3. MDC over RS-232

The same frame codec over a serial port, behind a `serial` feature
(`tokio-serial`), because video walls are frequently wired, not networked — and a
wired wall has no discovery, no DHCP and no TLS to argue about.

**Exit:** the codec is shared, not copied; `serial` is off by default so a cloud agent
does not compile a serial stack; the TCP path is unchanged.

## 4. `tv-rs-sony` — Bravia REST + IRCC

Sony's consumer and pro panels expose a documented JSON-REST API plus IRCC-IP for
key presses, authenticated with a pre-shared key. It is the smallest remaining vendor
integration: power, input, volume, app launch, and key codes.

**Exit:** passes the conformance suite; capability set is honest (no absolute volume
if the API only takes steps); pairing failure maps to `Auth` with `Remedy::Reprovision`.

## 5. Native CEC — only after a measurement

`tv-rs-cec` shells out to `cec-client`. That is a deliberate trade (no `unsafe`, no C
toolchain), and it is fine until someone shows the subprocess latency hurts. If that
measurement exists, add a `cec-native` feature that talks to `/dev/cec` with `unsafe`
confined to one module; keep the subprocess adapter as the default.

**Exit:** a number, not an opinion — p50/p95 for a standby command through each path,
and no new botched error paths.

## 6. DDC/CI in `tv-rs-local`

Brightness and input for the display the machine is *attached to*, over `/dev/i2c`
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

## 8. Release `0.3.0`

After 1–4: bump, changelog, tag, publish sixteen crates in order. The `msrv`,
`cargo-deny`, `per-crate features` and tokio-free gates already guard the mechanics.

---

## Explicitly rejected

Recorded so they do not come back as "suggestions" a year from now:

| Idea | Why not |
|---|---|
| `PowerState::PoweringOn/PoweringOff` as observable states | no consumer panel reports them; a TCP probe cannot tell "warming up" from "off", so the enum would turn an inference into a fact |
| `trait PowerControl: TvDevice` with `&mut self` | kills `Arc<dyn DeviceHandle>` and forces one lock per call; connection lifecycle belongs inside the backend |
| Stringly-typed errors (`AuthFailed(String)`) | removes `Remedy`, which is the only reason a caller can branch on what to *do* |
| `create_device(...) -> Box<dyn TvDevice>` with `todo!()` | unimplemented factory in a public API; the facade already dispatches on vendor |
| `type Percentage = u8` as a "bounded" type | an alias enforces nothing; a brightness control needs a validating newtype |
| A CMS/scheduler inside this workspace | `NON_GOALS.md`; the agent converges a device, it does not decide what plays |

## Definition of done for `0.3.0`

1. Telemetry landed, with one agent rule that acts on it.
2. `tv-rs-mdc` published, plus RS-232 behind a feature.
3. `tv-rs-sony` merged and conformance-passing.
4. `docs/hil-results.md` has at least one real row, or an explicit per-vendor
   "not verified" statement.
5. Every crate published at the same version; no new `unsafe` outside the two
   documented modules (`tv-rs-local`'s Windows FFI, and `tv-rs-cec` if native lands).
