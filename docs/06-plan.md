# Plan

**This is the only plan.** The specification documents that used to sit beside it
(`spec/00-device-model.md`, `spec/01-reconciliation.md`) are deleted: they described a
model that shipped, and a design that ships is documented by its code, its rustdoc and
[`architecture.md`](architecture.md), not by a proposal that keeps a phase table
forever. The P0–P9 proposals are archived in [`05-roadmap.md`](05-roadmap.md).

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
- **Telemetry is merged and unreleased** — `Telemetry`/`TelemetryProvider` in core,
  `Brightness`/`Reboot`/`Telemetry` capabilities, `Remedy::Quarantine`, the agent's
  `assess_telemetry` policy, and `DeviceOs::SamsungMdc`. It ships in the next version.
- **Hardware is still unverified.** `docs/hil-results.md` is empty and that is the one
  honest gap in the v0.2 definition of done.

---

## 1. Release `0.3.0`

Three breaking additions have accumulated on `main` since `0.2.0`: `Remedy::Quarantine`,
`DeviceOs::SamsungMdc`, and the three new capabilities. Under the policy in
`CONTRIBUTING.md` that is a **minor** bump, not a patch.

Order matters: `tv-rs-core` first (everything else resolves it from the registry), then
`tv-rs-transport`, then the protocol crates, `tv-rs-mdc` last among the leaves — it is
new, so its first upload is `0.3.0`. `cargo xtask publish --version 0.3.0` rewrites every
pin and publishes in that order.

**Why it blocks everything else:** `cargo package -p tv-rs-mdc` fails today because the
packaged tarball resolves `tv-rs-core 0.2.0` from crates.io, which has none of the new
APIs (`E0407`/`E0432`). The crate is fine; the registry is behind.

**Exit:** fourteen crates at `0.3.0`, a fresh `cargo add tv-rs` consumer builds against
`power_observation()` and `assess_telemetry`, docs.rs serves the new items, CHANGELOG
and README agree with the registry.

## 2. MDC over RS-232

The same frame codec over a serial port, behind a `serial` feature
(`tokio-serial`), because video walls are frequently wired, not networked — and a
wired wall has no discovery, no DHCP and no TLS to argue about.

**Exit:** the codec is shared, not copied; `serial` is off by default so a cloud agent
does not compile a serial stack; the TCP path is unchanged.

## 3. `tv-rs-sony` — Bravia REST + IRCC

Sony's consumer and pro panels expose a documented JSON-REST API plus IRCC-IP for
key presses, authenticated with a pre-shared key. It is the smallest remaining vendor
integration: power, input, volume, app launch, and key codes.

**Exit:** passes the conformance suite; capability set is honest (no absolute volume
if the API only takes steps); pairing failure maps to `Auth` with `Remedy::Reprovision`.

## 4. Native CEC — only after a measurement

`tv-rs-cec` shells out to `cec-client`. That is a deliberate trade (no `unsafe`, no C
toolchain), and it is fine until someone shows the subprocess latency hurts. If that
measurement exists, add a `cec-native` feature that talks to `/dev/cec` with `unsafe`
confined to one module; keep the subprocess adapter as the default.

**Exit:** a number, not an opinion — p50/p95 for a standby command through each path,
and no new botched error paths.

## 5. DDC/CI in `tv-rs-local`

Brightness and input for the display the machine is *attached to*, over `/dev/i2c`
(`VCP 0x10`, `VCP 0x60`). It belongs in `tv-rs-local` because that crate already owns
"this host's panel", and it must never be confused with controlling a remote panel.

**Exit:** Linux-only, feature-gated, reads VCP codes on a real monitor, and returns a
typed error where i2c is unavailable.

## 6. Hardware verification — the only unmet gate

`cargo xtask hw-test --write-doc` fills `docs/hil-results.md` the moment a television
answers SSDP. Until then every protocol claim in this repository is
documented-and-mocked.

**Exit:** at least one row per supported vendor (Tizen, webOS, Roku, MDC), each stating
what was commanded and what came back — or an explicit "not verified" line, which is
also a result.

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

## Definition of done for `0.4.0`

1. `tv-rs-mdc` published (it goes out with `0.3.0`), plus RS-232 behind a feature.
2. `tv-rs-sony` merged and conformance-passing.
3. `docs/hil-results.md` has at least one real row, or an explicit per-vendor
   "not verified" statement.
4. Every crate published at the same version; no new `unsafe` outside the two
   documented modules (`tv-rs-local`'s Windows FFI, and `tv-rs-cec` if native lands).
