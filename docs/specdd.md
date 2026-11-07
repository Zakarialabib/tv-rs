# SPECDD — tv-rs 0.7.1 Consolidation & Abstraction Refactor

> **Role:** single source of truth for the **tv-rs workspace only** — the crate topology audit, the `tv-drivers` consolidation plan, and the abstraction work that makes the workspace expandable without growing crate count linearly.
> **Version:** 3.0 · **Supersedes:** SPECDD v2.3 (2026-11-06) **for tv-rs scope only**.
> **Scope:** `tv-rs` facade + every published sub-crate at **`0.7.1`**. Companion-side consumption (`C-*` tasks), `cctv-rs`, and `mediamaster-rs` are **out of scope**; they stay in the companion SPECDD and their own roadmaps.
> **Audit basis:** docs.rs `tv-rs 0.7.1` re-export surface and GitHub `Zakarialabib/tv-rs` workspace description.
> **Repo rules:** `no_std` where it matters, **nothing panics in library code**, capability refusals are typed errors, no vendor SDK blobs, self-signed certs handled by pinning — not by disabling validation.

---

## 0. State audit — what `0.7.1` actually ships

The facade re-exports **17 sub-crates**:

| Crate | Current role | Consolidation candidate |
|---|---|---|
| `tv-rs` | facade: discovery orchestration, `TvClient` enum, `TvError`, `TvVendor`, `DiscoverySource` trait, composite discovery sources | **keep** — becomes the only public entry point |
| `tv-rs-core` | capability traits, `DeviceHandle`, `CommandId`, confidence-tagged state, error taxonomy, mock backend, conformance suite, quirks loader; **no tokio** | **keep** — the stable ABI |
| `tv-rs-discovery` | SSDP M-SEARCH, UPnP descriptor parsing | merge into `tv-drivers` (discovery feature) |
| `tv-rs-power` | WoL magic packets, TCP power probe | merge into `tv-drivers` (power feature) |
| `tv-rs-transport` | shared transport vocabulary (tokio-tungstenite WS, optional native-tls) | **keep as internal** — not a user-facing driver |
| `tv-rs-tizen` | Samsung Tizen remote channel + DIAL | merge into `tv-drivers` (tizen feature) |
| `tv-rs-webos` | LG webOS SSAP | merge into `tv-drivers` (webos feature) |
| `tv-rs-roku` | Roku ECP REST | merge into `tv-drivers` (roku feature) |
| `tv-rs-mdc` | Samsung commercial-panel MDC over TCP 1515 | merge into `tv-drivers` (mdc feature) |
| `tv-rs-sicp` | Philips SICP | merge into `tv-drivers` (sicp feature) |
| `tv-rs-ir` | Sony Bravia IRCC / IR | merge into `tv-drivers` (ir feature) |
| `tv-rs-cec` | HDMI-CEC (subprocess or native FFI) | merge into `tv-drivers` (cec feature) |
| `tv-rs-cast` | Google Cast sender (TLS + EKE2, protobuf) + DIAL fallback | merge into `tv-drivers` (cast feature) |
| `tv-rs-dlna` | DLNA renderer SSDP + AVTransport + GENA | **keep separate** — transport-shaped, used by `tv-rs-media` |
| `tv-rs-media` | HTTP 206 + HLS serving | **keep separate** — server, not a driver |
| `tv-rs-agent` | fleet reconciler, healing ladder, JSONL audit | **keep separate** — orchestration layer |
| `tv-rs-local` | local EDID / connector probe | **keep separate** — host probe, not a TV driver |
| `tv-rs-mirror` | video-session vocabulary (phone → panel) | **keep separate** — vocabulary, not a driver |
| `tv-rs-rag` | bounded ledger / retrieval kernel | **keep separate** — no protocol surface |
| `tv-rs-spatial-audio` | spatial-audio vocabulary | **keep separate** — media, not control |

**The problem this spec solves:** the workspace has grown to **~19 crates**. Protocol crates are one-per-vendor (`tizen`, `webos`, `roku`, `mdc`, `sicp`, `ir`, `cec`, `cast`) and each has its own error mapping, its own capability registration, and its own connection bootstrap. Adding a ninth vendor means adding a ninth crate, a ninth re-export, and a ninth place to remember to register capabilities. The `tv-drivers` consolidation collapses the **protocol-vendor axis** into one crate with feature-gated drivers, while leaving the cross-cutting axes (`core`, `agent`, `media`, `dlna`, `mirror`, `rag`, `local`, `spatial-audio`) as separate crates because they are **not** drivers.

---

## 1. Layer boundary (tv-rs internal)

| Layer | Crates | Rule |
|---|---|---|
| **Facade** | `tv-rs` | only public entry point; re-exports `drivers`, `core`, `agent`, `media`, `dlna`, `mirror`, `rag`, `local`, `spatial-audio` |
| **Device model** | `tv-rs-core` | `no_std` + `alloc`; no tokio; capability traits, `DeviceHandle`, `CommandId`, error taxonomy, mock backend, conformance suite, quirks loader |
| **Drivers** | `tv-rs-drivers` (new, replaces `tizen`/`webos`/`roku`/`mdc`/`sicp`/`ir`/`cec`/`cast` + `discovery`/`power`) | feature-gated per protocol; implements `Driver` trait from `tv-rs-core`; owns its transport bootstrap and its error → `TvError` mapping |
| **Transport vocabulary** | `tv-rs-transport` | internal only; shared WS/TLS vocabulary for drivers that need it; not re-exported as a driver |
| **Orchestration** | `tv-rs-agent` | consumes `DeviceHandle` only; never vendor-specific |
| **Media & vocabulary** | `tv-rs-media`, `tv-rs-dlna`, `tv-rs-mirror`, `tv-rs-rag`, `tv-rs-spatial-audio` | not drivers; keep independent version cadence |
| **Host probe** | `tv-rs-local` | not a TV driver; EDID/connector for the host's own screen |

**Dependency law (new):** `tv-rs-core` depends on nothing in the workspace. `tv-rs-drivers` depends only on `tv-rs-core` and `tv-rs-transport`. `tv-rs` depends on everything it re-exports. `tv-rs-agent` depends on `tv-rs-core` **only** — never on `tv-rs-drivers` — so the reconciler cannot accidentally grow vendor branches.

---

## 2. The `tv-rs-drivers` consolidation

### 2.1 What moves

| From | To | Feature flag | Kept surface |
|---|---|---|---|
| `tv-rs-tizen` | `tv-rs-drivers::tizen` | `tizen` | `TizenRemote`, pairing-token store, DIAL launch |
| `tv-rs-webos` | `tv-rs-drivers::webos` | `webos` | `WebOsRemote`, `client-key` persistence, SSAP |
| `tv-rs-roku` | `tv-rs-drivers::roku` | `roku` | `RokuEcp`, app listing, key presses |
| `tv-rs-mdc` | `tv-rs-drivers::mdc` | `mdc` | `MdcClient`, video-wall by display ID, panel health |
| `tv-rs-sicp` | `tv-rs-drivers::sicp` | `sicp` | `SicpClient` |
| `tv-rs-ir` | `tv-rs-drivers::ir` | `ir` | `IrccClient` |
| `tv-rs-cec` | `tv-rs-drivers::cec` | `cec` | CEC subprocess / native FFI behind `cec-native` |
| `tv-rs-cast` | `tv-rs-drivers::cast` | `cast` | `CastClient`, `media_status()`, transport controls |
| `tv-rs-discovery` | `tv-rs-drivers::discovery` | `discovery` (default) | `SsdpDiscoverySource`, `CompositeDiscoverySource`, vendor probes |
| `tv-rs-power` | `tv-rs-drivers::power` | `power` (default) | WoL, TCP power probe |

### 2.2 What stays separate (and why)

- **`tv-rs-dlna`** — DLNA is a renderer transport with SSDP + AVTransport + GENA; it is consumed by `tv-rs-media` and by the companion's cast surface, not by `TvClient` key dispatch.
- **`tv-rs-media`** — HTTP 206 / HLS is a server, not a control channel.
- **`tv-rs-agent`** — reconciler must remain vendor-blind; it consumes `DeviceHandle` only.
- **`tv-rs-local`** — EDID probe is for the host's own screen, not a remote TV.
- **`tv-rs-mirror`, `tv-rs-rag`, `tv-rs-spatial-audio`** — vocabulary/kernel crates with independent version cadence.

### 2.3 The new abstraction: `Driver` trait

Today each protocol crate exposes its own client type and its own error enum. The consolidation introduces **one trait** in `tv-rs-core`:

```rust
pub trait Driver: Send + Sync {
    type Handle: DeviceHandle;
    fn vendor(&self) -> TvVendor;
    fn capabilities(&self) -> &CapabilitySet;
    async fn connect(&mut self, target: &DiscoveredTv) -> Result<Self::Handle, TvError>;
    async fn send(&mut self, cmd: Command) -> Result<CommandAck, TvError>;
    async fn observe(&mut self) -> Result<ObservationStream, TvError>;
}
```

Every vendor implements `Driver`; the facade's `TvClient` becomes a thin enum over `Box<dyn Driver>` and no longer pattern-matches on vendor for key dispatch. This is the single change that makes the workspace **expandable**: adding a tenth vendor is "implement `Driver`, add one feature flag, register in `tv-drivers::registry()`" — not "add a crate, add a re-export, add an error mapping, add a capability registration."

### 2.4 The registry

`tv-rs-drivers::registry()` returns `Vec<Box<dyn Driver>>` filtered by enabled features. The facade uses it for:

- discovery union (`CompositeDiscoverySource` already exists; it now pulls from the registry);
- `TvClient` dispatch (no more `SamsungRemoteState`-only path);
- capability aggregation (`CapabilitySet::union` across drivers);
- conformance suite selection (each driver ships a `ConformanceCase` the core test harness runs).

---

## 3. Abstraction improvements (beyond consolidation)

### 3.1 Unified `CapabilitySet` registration

Today capability sets are per-vendor and assembled by hand. The refactor makes `Driver::capabilities()` the **only** source; `tv-rs-core` gains `CapabilitySet::from_driver(&dyn Driver)` and a `CapabilityRegistry` that the conformance suite uses to assert every advertised capability has at least one conformance case.

### 3.2 Unified `Error → Remedy` mapping

Each protocol crate maps its own error to `TvError`. The refactor moves mapping into `tv-rs-drivers::map::<vendor>::to_tv_error()` — one module per vendor, all behind the same `TvError` enum. The companion's remedy map then has **one** source of truth.

### 3.3 Observation channel, not polling

`tv-rs-agent` currently polls (`pulse.rs` in the companion is the downstream symptom). The refactor promotes `Driver::observe()` to the only state-change path; the facade exposes `TvClient::observe() -> impl Stream<Item = TvEvent>`; the agent subscribes instead of ticking.

### 3.4 `no_std` boundary made explicit

`tv-rs-core` already claims "no tokio in its graph, ever". The refactor adds a CI gate: `cargo check -p tv-rs-core --no-default-features --features alloc --target thumbv7em-none-eabihf`. Drivers are **not** `no_std` (they speak sockets); only the model is.

### 3.5 Version cadence

`tv-rs-core` gets a **stable 1.0-track** (`0.8.x` → `1.0` once `Driver` lands); `tv-rs-drivers` follows the facade; `tv-rs-agent`, `tv-rs-media`, `tv-rs-dlna`, `tv-rs-mirror`, `tv-rs-rag`, `tv-rs-spatial-audio`, `tv-rs-local` keep independent minor cadences. This ends the current pattern where every crate bumps in lockstep because one vendor needs a fix.

---

## 4. Task cards (Jules-style, tv-rs only)

Each card: **id · branch · context · files · steps · verification · PR title**.

### TV-10 — Create `tv-rs-drivers` skeleton with `Driver` trait
**Repo:** `tv-rs` @ `feat/tv10-drivers-skeleton`
**Context:** the consolidation needs a home before any vendor moves.
**Files:** new `crates/drivers/Cargo.toml`, `crates/drivers/src/lib.rs`, `crates/core/src/driver.rs`
**Steps:** define `Driver` trait in `tv-rs-core` (§2.3); create `tv-rs-drivers` with empty feature modules `tizen`, `webos`, `roku`, `mdc`, `sicp`, `ir`, `cec`, `cast`, `discovery`, `power`; add `registry()` returning an empty `Vec` gated by features.
**Verification:** `cargo check -p tv-rs-drivers --no-default-features` green; `cargo check -p tv-rs-drivers --all-features` green; `tv-rs-core` still `no_std`-clean.
**PR:** `feat(core): Driver trait + tv-rs-drivers skeleton`

### TV-11 — Move `tv-rs-discovery` and `tv-rs-power` into `tv-drivers`
**Repo:** `tv-rs` @ `refactor/tv11-discovery-power`
**Context:** both are protocol-adjacent and small enough to fold.
**Files:** `crates/discovery/`, `crates/power/`, `crates/drivers/src/discovery/`, `crates/drivers/src/power/`, workspace `Cargo.toml`
**Steps:** move code into `tv-rs-drivers` behind `discovery` (default) and `power` (default); keep the old crates as deprecated shims that re-export for one release; update `tv-rs` facade re-exports.
**Verification:** facade `cargo test` green; a scratch consumer using `tv_rs::discovery::*` still compiles via the shim; `cargo publish --dry-run` per moved crate is **not** attempted (shims are unpublished).
**PR:** `refactor(drivers): fold discovery and power into tv-rs-drivers`

### TV-12 — Move `tv-rs-tizen` and `tv-rs-webos` into `tv-drivers`
**Repo:** `tv-rs` @ `refactor/tv12-tizen-webos`
**Context:** the two WebSocket vendors share `tv-rs-transport`; they belong behind one feature pair.
**Files:** `crates/tizen/`, `crates/webos/`, `crates/drivers/src/tizen/`, `crates/drivers/src/webos/`
**Steps:** move clients; implement `Driver` for each; wire `TizenRemote`/`WebOsRemote` as `Handle` types; keep `pairing-token` and `client-key` stores inside the driver; remove per-crate error maps in favour of `tv-rs-drivers::map::tizen::to_tv_error` and `…::webos::…`.
**Verification:** `cargo test -p tv-rs-drivers --features tizen,webos` green; the facade's `TvClient` no longer calls `SamsungRemoteState` directly.
**PR:** `refactor(drivers): fold tizen and webos into tv-rs-drivers`

### TV-13 — Move `tv-rs-roku`, `tv-rs-mdc`, `tv-rs-sicp`, `tv-rs-ir`
**Repo:** `tv-rs` @ `refactor/tv13-consumer-and-panel`
**Context:** four more vendors, three of them small.
**Files:** `crates/roku/`, `crates/mdc/`, `crates/sicp/`, `crates/ir/`, corresponding `crates/drivers/src/*`
**Steps:** same shape as TV-12; for `mdc`, keep the `MdcClient` panel-health / video-wall surface intact; for `ir`, keep the IRCC / Scalar split as internal modules.
**Verification:** `cargo test -p tv-rs-drivers --features roku,mdc,sicp,ir` green; `MdcClient` video-wall-by-display-ID test passes.
**PR:** `refactor(drivers): fold roku, mdc, sicp, ir into tv-rs-drivers`

### TV-14 — Move `tv-rs-cast` and `tv-rs-cec`
**Repo:** `tv-rs` @ `refactor/tv14-cast-cec`
**Context:** `GoogleCast` currently returns `UnsupportedDevice` in `TvClient` and CEC is re-exported but not driven — both are honesty gaps that the consolidation naturally fixes.
**Files:** `crates/cast/`, `crates/cec/`, `crates/drivers/src/cast/`, `crates/drivers/src/cec/`
**Steps:** implement `Driver` for `CastClient` and `CecClient`; remove the `UnsupportedDevice` shim; route `TvClient::send_key/launch/power` through the driver registry.
**Verification:** a `TvClient` obtained from a cast device dispatches `power_off` and `launch`; a `TvClient` obtained from a CEC device dispatches `power_on`/`standby`; `cargo test -p tv-rs-drivers --features cast,cec` green.
**PR:** `refactor(drivers): fold cast and cec; remove UnsupportedDevice shim`

### TV-15 — Promote `Driver::observe()` through `TvClient`
**Repo:** `tv-rs` @ `feat/tv15-observe`
**Context:** the agent and the companion both poll today.
**Files:** `crates/core/src/observe.rs`, `crates/drivers/src/registry.rs`, `crates/tv-rs/src/client.rs`
**Steps:** add `ObservationStream`; implement per driver with a `tokio::sync::broadcast`; `TvClient::observe()` returns the stream; ordering guaranteed per device.
**Verification:** 3-vendor integration test (tizen + roku + mdc): state change → event ≤200 ms; backpressure drops oldest, not newest.
**PR:** `feat(core): Driver::observe and TvClient::observe`

### TV-16 — Capability registry + conformance suite from drivers
**Repo:** `tv-rs` @ `feat/tv16-capability-registry`
**Context:** capability sets and conformance cases are currently per-crate and hand-wired.
**Files:** `crates/core/src/capability.rs`, `crates/drivers/src/conformance.rs`
**Steps:** `CapabilitySet::from_driver`; `Driver::conformance_cases()`; `tv-rs-core` conformance harness iterates the registry; a capability with no case is a test failure.
**Verification:** `cargo test -p tv-rs-core --features conformance` runs the suite across every enabled driver and fails if any capability is orphaned.
**PR:** `feat(core): capability registry + driver-driven conformance suite`

### TV-17 — `no_std` CI gate for `tv-rs-core`
**Repo:** `tv-rs` @ `ci/tv17-nostd`
**Context:** the README claims "no tokio in its graph, ever"; nothing enforces it.
**Files:** `.github/workflows/ci.yml`
**Steps:** add `cargo check -p tv-rs-core --no-default-features --features alloc --target thumbv7em-none-eabihf`; block merge if red; also assert `cargo tree -p tv-rs-core | grep tokio` is empty.
**Verification:** matrix green; deliberately adding `use tokio::…` to core makes it red.
**PR:** `ci: enforce no_std + no-tokio boundary for tv-rs-core`

### TV-18 — Feature-matrix gate for `tv-rs-drivers`
**Repo:** `tv-rs` @ `ci/tv18-feature-matrix`
**Context:** `0.6.0` shipped with missing features because nothing checked combinations.
**Files:** `.github/workflows/feature-matrix.yml`
**Steps:** matrix over `{tizen, webos, roku, mdc, sicp, ir, cec, cast, discovery, power, all}`; `cargo check -p tv-rs-drivers --features $f` per cell; also `cargo publish --dry-run -p tv-rs-drivers --features all` on tag.
**Verification:** matrix green; removing a feature from `Cargo.toml` makes exactly one cell red.
**PR:** `ci: feature matrix gate for tv-rs-drivers`

### TV-19 — Version cadence: core on 1.0-track, drivers follow facade
**Repo:** `tv-rs` @ `release/tv19-cadence`
**Context:** today everything bumps in lockstep.
**Files:** workspace `Cargo.toml`, per-crate `Cargo.toml`, `CHANGELOG.md` per crate
**Steps:** declare `tv-rs-core` at `0.8.0` heading to `1.0`; `tv-rs-drivers` at `0.8.0` following the facade; `tv-rs-agent`, `tv-rs-media`, `tv-rs-dlna`, `tv-rs-mirror`, `tv-rs-rag`, `tv-rs-spatial-audio`, `tv-rs-local` at independent `0.x` minors; document the policy in `docs/versioning.md`.
**Verification:** `cargo metadata` shows the intended versions; changelogs exist per crate.
**PR:** `release: split version cadences (core 1.0-track, drivers follow facade)`

### TV-20 — HIL rows for every consolidated driver
**Repo:** `tv-rs` @ `test/tv20-hil-rows`
**Context:** the README admits no hardware verification and an empty HIL table.
**Files:** `docs/08-hil-results.md`, `tests/hil/*.rs`
**Steps:** one row per driver (tizen, webos, roku, mdc, sicp, ir, cec, cast, discovery, power); device + firmware + pass/fail; gate behind `--features hil`.
**Verification:** `cargo test --features hil` prints the table; every row references a device or a mock with a documented reason.
**PR:** `test(hil): rows for every consolidated driver`

### TV-21 — Fix `tv-rs-vision` synthetic frames (honesty)
**Repo:** `tv-rs` @ `fix/tv21-vision`
**Context:** `stream.rs:105` returns `VideoFrame::synthetic(..)` — it decodes nothing. This is listed in the honesty table and has been deferred long enough.
**Files:** `crates/vision/src/stream.rs`
**Steps:** either wire a real H.264/H.265 decoder or remove `tv-rs-vision` from `TvClient` re-exports and set `publish = false` until a decoder exists.
**Verification:** no `VideoFrame::synthetic` reachable from the public API; `cargo test -p tv-rs-vision` green.
**PR:** `fix(vision): remove synthetic-frame path from public API`

---

## 5. Gap index (tv-rs only; `G`-ids preserved where they existed)

| ID | State | Task | Hint |
|---|---|---|---|
| G05 | ⏳ | TV-05 (old) | Tizen `ed.installedApp.get` + foreground/mute REST — folded into `tv-drivers::tizen` |
| G18 | ⏳ | **TV-15** | long-lived observation instead of polling |
| G24a | ⏳ | **TV-14** | `CastClient` transport undriveable → `Driver` implementation fixes it |
| G27 | ⏳ | TV-06 (old) | `tv-rs-provision` PR #99 — unrelated to consolidation; track separately |
| **G28** | ⚠️ re-scoped | **TV-18** | umbrella feature set was incomplete at `0.6.0`; `0.7.1` appears complete but the feature-matrix gate prevents recurrence |
| **G34** | ⏳ | **TV-10 … TV-13** | protocol-vendor crates are one-per-vendor; consolidation into `tv-drivers` |
| **G35** | ⏳ | **TV-16** | capability sets and conformance cases are hand-wired per crate |
| **G36** | ⏳ | **TV-17** | `no_std`/no-tokio boundary for `tv-rs-core` is claimed but not CI-enforced |
| **G37** | ⏳ | **TV-19** | lockstep versioning makes every fix bump every crate |
| **G38** | ⏳ | **TV-20** | HIL table is empty |
| **G39** | ⏳ | **TV-21** | `tv-rs-vision` synthetic frames |

**Workaround ladder:** thin seams only; annotate every workaround `// GAP G<n>`; delete the workaround once the consolidated driver ships.

---

## 6. Honesty — do not wire as real (tv-rs only)

| Location | Claim | Truth | Task |
|---|---|---|---|
| `tv-rs-vision` `stream.rs:105` | decodes video | `VideoFrame::synthetic(..)` — decodes nothing | **TV-21** |
| `TvVendor::GoogleCast` | supported in `TvClient` | returns `TvError::UnsupportedDevice` | **TV-14** |
| `tv-rs-cec` facade re-export | CEC drivable via `TvClient` | re-exported but not driven | **TV-14** |
| Cast transport | play/pause/seek work | `CastClient` has no `media_status()` in `TvClient` | **TV-14** |
| HIL table | hardware verified | empty | **TV-20** |

**Rule:** every row must have a matching `// GAP G<n>` comment at the code site and a task above.

---

## 7. Sequencing (dependency-ordered, agent-runnable)

### Wave 0 — Skeleton (this week)
- **TV-10** (`Driver` trait + `tv-rs-drivers` skeleton)
- **TV-17** (`no_std` CI gate — catches regressions from the skeleton)
- **TV-18** (feature-matrix gate — catches missing features from day one)

### Wave 1 — Fold the small drivers
- **TV-11** (discovery + power) → **TV-13** (roku + mdc + sicp + ir)
- Deprecated shims keep the facade compiling throughout.

### Wave 2 — Fold the WebSocket and transport drivers
- **TV-12** (tizen + webos) — exercises `tv-rs-transport` from inside `tv-rs-drivers`
- **TV-14** (cast + cec) — removes the `UnsupportedDevice` shim and the CEC re-export gap

### Wave 3 — Abstractions that only pay off after consolidation
- **TV-15** (`observe`) — needs `Driver` from TV-10 and at least three drivers from TV-12/13
- **TV-16** (capability registry + conformance) — needs every driver behind `Driver`

### Wave 4 — Release hygiene and evidence
- **TV-19** (version cadence)
- **TV-20** (HIL rows)
- **TV-21** (`tv-rs-vision` honesty fix)

---

## 8. Errata & history

**Superseded for tv-rs scope:** SPECDD v2.3 (`TV-01 … TV-09` were written against `0.6.0`; `TV-01` is obsolete because `0.7.1` resolves the umbrella feature gap). `TV-06` (`tv-rs-provision` PR #99) remains valid and is tracked outside this consolidation spec.

**Corrections carried forward:**
- `G28` was over-stated as “all cargo gates blocked”; the companion pins the **sub-crates**, not the umbrella, so its build was never blocked. The feature-matrix gate (**TV-18**) is the durable fix.
- The `tv-rs` README states “thirteen installable crates” and “`tv-rs-mdc` merged but not published” — that reflects an older version page; `0.7.1` re-exports **17 sub-crates** on docs.rs. The consolidation spec uses the `0.7.1` topology.

**History:**
- **v3.0** (2026-11-07) — tv-rs-only reframe at `0.7.1`. `tv-drivers` consolidation plan (`TV-10 … TV-13`), `Driver` trait, observation channel, capability registry, `no_std` gate, feature-matrix gate, version cadence, HIL rows, vision honesty fix. Companion `C-*` tasks and `cctv`/`mediamaster` tasks removed to their own specs.
- **v2.3** (2026-11-06) — companion-first SPECDD; `C-08` landed, `G28` re-scoped, `G29` closed.

---

## TL;DR for you as PM

`0.7.1` re-exports **17 sub-crates** — eight of them are protocol vendors, each a separate crate with its own error map, its own capability registration, and its own connection bootstrap. That is why adding a ninth vendor feels like a new project instead of a feature.

The refactor is one idea — **`tv-rs-drivers`** — plus the abstractions that make it pay off:

- **Consolidate** `tizen`, `webos`, `roku`, `mdc`, `sicp`, `ir`, `cec`, `cast`, `discovery`, `power` behind one crate with feature flags (`TV-10 … TV-14`).
- **Abstract** the vendor axis with a `Driver` trait, a registry, and a `TvClient` that no longer pattern-matches on vendor (`TV-10`, `TV-15`, `TV-16`).
- **Make it expandable** by making “add a vendor” equal to “implement `Driver` + one feature flag”, enforced by a feature-matrix gate (`TV-18`) and a capability registry that refuses orphan capabilities (`TV-16`).
- **Fix the honesty gaps that the consolidation naturally exposes**: cast transport, CEC dispatch, `tv-rs-vision` synthetic frames (`TV-14`, `TV-21`).

Keep `tv-rs-agent`, `tv-rs-media`, `tv-rs-dlna`, `tv-rs-local`, `tv-rs-mirror`, `tv-rs-rag`, and `tv-rs-spatial-audio` as separate crates — they are not drivers, they are orchestration, server, vocabulary, or host-probe layers. Consolidating them would recreate the same problem on a different axis.

Run **Wave 0** first: `TV-10` (skeleton), `TV-17` (`no_std` gate), `TV-18` (feature matrix). Those three are the guardrails that make the rest of the refactor safe to land driver-by-driver.