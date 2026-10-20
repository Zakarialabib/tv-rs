# Spec: the v2 device model

**Status:** **shipped** · written 2026-10-19, implemented and released as `0.2.0` ·
this page is now the reference for the model, not a plan. Open work moved to
[`../06-plan.md`](../06-plan.md).

This is the specification for turning the current workspace — eight protocol crates
that each speak their wire format well — into a **device model**: a small, embeddable
core that a fleet manager, a signage CMS or an installer's tool can drive without
knowing which vendor it is talking to.

It is written against the code that exists today, not against a greenfield
assumption. Where the reviewed playbook and the repository disagree, the
disagreement is recorded in [`01-reconciliation.md`](01-reconciliation.md) and the
resolution is stated here as the normative rule.

---

## 1. The claim being made

> A TV is a device with **capabilities**, a **state you cannot fully observe**, and a
> **network that fails in ordinary ways**. A library that models only the first —
> "here are the endpoints for a Samsung TV" — pushes the other two onto every
> consumer, and every consumer writes them differently.

So the core sells three things, not one:

1. **Capability dispatch** — `Arc<dyn DeviceHandle>`, no generics in user types.
2. **Epistemic honesty** — state carries a confidence level, because a TV that has
   not answered in five minutes is neither known-on nor known-off.
3. **Boring failure semantics** — an error taxonomy that routes: this means
   re-pair, this means reconnect, this means stop trying.

Everything else (protocols, ports, keys, frames) stays in backend crates and stays
as good as it is today.

## 2. Non-goals

Restated at the top because this is where such projects die.

- **Not a CMS, scheduler, playlist engine or player.** The agent converges a device
  to a desired state; it does not decide what should play.
- **Not a transcoder or media server.** `tv-rs-media` serves bytes; it does not
  encode them.
- **Not a cloud service.** LAN-only by design; no accounts, no relay, no telemetry.
- **Not a universal remote UI.** Keys exist because automation needs them to
  recover a device, not to replace a handset.
- **Not CEC-first.** Panel control over HDMI-CEC requires hardware nobody has on a
  server; it is an optional crate, off by default, and never presented as the fix.

## 3. Workspace contract

```
crates/
  tv-rs                 facade: TvClient enum, discovery orchestration  (exists)
  tv-rs-core            NEW: capability traits, DeviceHandle, CommandId,
                        ObservedState, TvError taxonomy, mock backend
  tv-rs-discovery       SSDP + UPnP descriptors                        (exists)
  tv-rs-power           Wake-on-LAN + TCP probe                        (exists)
  tv-rs-transport       TLS policy/pinning + frame reader              (exists)
  tv-rs-roku            Roku ECP                                       (exists)
  tv-rs-webos           LG SSAP                                        (exists)
  tv-rs-tizen           Samsung remote channel + DIAL                  (exists)
  tv-rs-credentials     NEW: CredentialStore trait + file backend
  tv-rs-media           206 + HLS server                               (exists)
  tv-rs-agent           NEW: Reconciler / Healer / Provisioner (lib + bin)
  tv-rs-cec             NEW, optional: libcec/cec-client behind `cec`
  tv-rs-cli             the reference consumer                         (exists)
xtask/                  cargo xtask publish | hw-test | quirks-check
quirks/                 versioned model+firmware → fallback rules
```

**Normative rules**

1. `tv-rs-core` compiles with `--no-default-features` and **must not depend on
   tokio** in that configuration. Time types in the core are `std::time::Instant`;
   tokio appears only behind `runtime-tokio`/`mock-backend` features.
2. Backend crates depend on `tv-rs-core` for types only. They never depend on each
   other, and never on `tv-rs-agent`.
3. `tv-rs-agent` depends on core + backends, and on nothing that a consumer must
   also adopt — its providers are traits with three small implementations.
4. Every crate keeps `[lints] workspace = true`, `#![forbid(unsafe_code)]` except
   `tv-rs-cec`.

**Deviations from the reviewed playbook** (named, not hidden):

| Playbook | Spec | Why |
|---|---|---|
| `tv-core`, `tv-roku`, `tv-lg`, `tv-samsung`, `tv-agent`, `tv-cli` | `tv-rs-core`, `tv-rs-roku`, `tv-rs-webos`, `tv-rs-tizen`, `tv-rs-agent`, `tv-rs-cli` | one prefix across eight existing crates; `tv-cli` is already taken on crates.io (verified 2026-10-19), `tv` and `tv-cli` are the only unavailable names in the set |
| `ObservedField::last_verified_at: tokio::time::Instant` | `std::time::Instant` | keeps `tv-core` tokio-free, which the same playbook requires |
| LG accept-invalid TLS behind a flag | `TlsPolicy::Pinned` / `TrustOnFirstUse` | already shipped in `tv-rs-transport`; the escape hatch is not the only option any more |
| Dual MIT/Apache-2.0 | MIT (unchanged) | the repo is MIT today; relicensing is a governance decision, not a refactor — see §11 |

## 4. `tv-rs-core` — public API

### 4.1 Capability traits

Dyn-compatible by construction; `async-trait` is mandatory because native `async fn`
in traits is not dyn-compatible and the fleet stores `Arc<dyn DeviceHandle>`.

```rust
#[async_trait]
pub trait PowerControl: Send + Sync {
    async fn power_state(&self) -> Result<PowerState, TvError>;
    async fn wake(&self) -> Result<(), TvError>;   // WoL; Unsupported, never a fake
    async fn sleep(&self) -> Result<(), TvError>;
}

#[async_trait]
pub trait KeyInput: Send + Sync {
    async fn key_press(&self, key: Key, cmd: CommandId) -> Result<(), TvError>;
}

#[async_trait]
pub trait AppLaunch: Send + Sync {
    async fn launch(&self, app: AppRef, cmd: CommandId) -> Result<(), TvError>;
    async fn foreground(&self) -> Result<Option<AppRef>, TvError>;
}

#[async_trait]
pub trait InputSelect: Send + Sync {
    async fn active_input(&self) -> Result<Option<Input>, TvError>;
    async fn select_input(&self, input: Input, cmd: CommandId) -> Result<(), TvError>;
}

#[async_trait]
pub trait VolumeControl: Send + Sync {
    async fn volume(&self) -> Result<Option<u8>, TvError>;
    async fn set_volume(&self, level: u8, cmd: CommandId) -> Result<(), TvError>;
    async fn set_mute(&self, muted: bool, cmd: CommandId) -> Result<(), TvError>;
}
```

`VolumeControl` is an addition to the reviewed list. Without it every consumer
writes `match vendor` for the single most common operation, which is exactly what
the core exists to prevent; three of the five vendors support it today.

### 4.2 The erased handle

```rust
pub trait DeviceHandle: Send + Sync {
    fn id(&self) -> DeviceId;
    fn descriptor(&self) -> &DeviceDescriptor;   // os, model, firmware, mac, address
    fn capabilities(&self) -> &CapabilitySet;    // cached manifest
    fn as_power(&self) -> Option<&dyn PowerControl>;
    fn as_key(&self) -> Option<&dyn KeyInput>;
    fn as_app(&self) -> Option<&dyn AppLaunch>;
    fn as_input(&self) -> Option<&dyn InputSelect>;
    fn as_volume(&self) -> Option<&dyn VolumeControl>;
}
```

`None` from an accessor means "this device cannot do it", which is the same fact as
`TvError::Unsupported` — but discoverable *before* a command is attempted. A Roku
streaming stick has no panel, so `as_input()` returning `None` is the correct answer
and a fleet UI should grey the control out rather than send a command and fail.

**Relationship to today's `TvClient`.** The enum stays. It is the natural concrete
type for the three backends the facade dispatches to, its `match` is exhaustive at
compile time, and it is what a single-TV consumer should use. It implements
`DeviceHandle` (via a thin adapter per vendor), so both worlds coexist:

- single TV, known vendor → `TvClient`
- fleet of unknown shape → `Arc<dyn DeviceHandle>`

### 4.3 `CommandId` and idempotency

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct CommandId(Uuid);   // caller- or agent-generated, opaque to backends
```

Every mutating method takes one. **Backends do not deduplicate**; the dedup window
is a deployment property (30 s for app launch, ~5 s for keypresses) and belongs to
the layer that owns retries. The core states the contract: *"two deliveries of one
`CommandId` must have the effect of one."*

### 4.4 Confidence and observed state

```rust
pub enum Confidence { Unknown, Inferred, High }

pub struct ObservedField<T> {
    pub value: Option<T>,
    pub confidence: Confidence,
    pub last_verified_at: Instant,   // std::time::Instant
}

pub struct ObservedState {
    pub power: ObservedField<PowerState>,
    pub input: ObservedField<Input>,
    pub foreground: ObservedField<AppRef>,
    pub volume: ObservedField<u8>,
}
```

`tv-rs-core` stores the shape and **holds no policy**. Decay is an agent concern:

```rust
pub struct DecayPolicy {                 // lives in tv-rs-agent
    pub high_to_inferred: Duration,      // default 120 s
    pub inferred_to_unknown: Duration,   // default 300 s
}
```

Rules the backends must honour when filling these in:

- **A read sets `High`** — the device told us.
- **A write that the protocol acknowledges with a state change sets `High`** (LG's
  `setVolume` returns the new state).
- **A write with only a 200 sets `Inferred`** — Roku `POST /launch/{id}` and Samsung
  `ed.apps.launch` both return success without proving anything.
- **A toggle sets `Inferred` at best** — `KEY_POWER` is not a set-to-off command.

### 4.5 Error taxonomy (frozen at v0.2)

```rust
#[derive(Debug, thiserror::Error)]
pub enum TvError {
    #[error("transport: {0}")]       Transport(TransportError),   // unreachable, timeout, TLS
    #[error("protocol: {0}")]        Protocol(ProtocolError),     // malformed / unexpected reply
    #[error("auth: {0}")]            Auth(AuthError),             // PairingRequired, TokenExpired, NotPermitted
    #[error("state: {0}")]           State(StateError),           // NotReady, Quarantined
    #[error("unsupported: {0}")]     Unsupported(Capability),
    #[error("platform quirk: {0}")]  PlatformQuirk(QuirkId),
}
```

It is a **routing table**, not a description:

| Variant | Caller response |
|---|---|
| `Transport` | retry with backoff, then Healer |
| `Auth` | re-provision (pair again); never retry blindly |
| `Protocol` | log with the raw payload, back off, then quarantine |
| `State` | wait or stop; do not treat as transport failure |
| `Unsupported` | disable the control in the UI |
| `PlatformQuirk` | consult `quirks/`, apply the documented fallback |

Adding or removing a top-level variant is a **breaking change**. The current
per-crate errors (`RokuError`, `TizenError`, `WebosError`, …) stay as the
implementations' detail and are converted at the trait boundary, so no consumer sees
them through `DeviceHandle`.

### 4.6 Mock backend (`feature = "mock-backend"`)

A scriptable backend used by every phase from 1 onward, able to simulate:

- synchronous request/response (Roku-shaped) and asynchronous push (LG/Samsung-shaped);
- timeout **before** send, timeout **after** send (ack lost — the most common real
  failure), malformed response, auth expiry mid-session, power-off mid-command;
- per-device capability sets, so "Roku stick has no input" is testable.

## 5. Backend conformance

One suite lives in `tv-rs-core/tests/conformance.rs` and every backend runs it:

| Obligation | Pass condition |
|---|---|
| Capability honesty | every `as_*()` accessor agrees with `CapabilitySet`; an unsupported call returns `Unsupported`, never a silent `Ok` |
| Confidence discipline | reads report `High`; toggle writes never report `High`; a failed write invalidates the field |
| Error routing | unreachable → `Transport`; bad token → `Auth`; unknown firmware string → `Protocol` or `PlatformQuirk`, never a panic |
| Idempotency | the suite replays a `CommandId` and asserts the backend performs one physical action |
| Ladder | where a vendor has port generations, the negotiated one is recorded in the descriptor and reused |

Per-vendor specifics that the spec fixes now (they are the ones most likely to be
"fixed" wrongly later):

- **Roku**: `power-mode` exists on Roku TVs, not players; `POST /launch` returning
  200 proves nothing (`Inferred`); Roku OS 14.1+ requires *Settings → System →
  Advanced system settings → Control by mobile apps* for writes → `Auth(NotPermitted)`
  with the setting named in the error.
- **webOS**: 2023+ firmware may accept only `wss://:3001`; pair once, persist
  `client-key`, and treat a prompt on every connect as `Auth(PairingRequired)`.
- **Tizen**: port ladder 8002 → 8001 (legacy 55000 is out of scope — no tested
  device); `KEY_POWER` is a toggle; app launch needs DIAL or the remote channel's
  launch payload, and success is `Inferred`.

## 6. `tv-rs-agent`

### 6.1 The seam that keeps the core honest

```rust
#[async_trait]
pub trait DesiredStateProvider: Send + Sync {
    async fn desired_state(&self, device: &DeviceDescriptor) -> Result<Option<DesiredState>, ProviderError>;
    fn watch(&self) -> Option<BoxStream<'static, DesiredStateEvent>> { None }
}
```

Three implementations: `StaticFileProvider` (TOML/JSON), `HttpPollProvider` (GET a
manifest on an interval), `MqttSubscribeProvider` (feature `mqtt`). `tv-core` must
not know any of them exists.

### 6.2 Reconciler

Per device, on a jittered interval: fetch desired → decay confidence → probe only
what the delta needs → converge → schedule verification. It **must not** use
`KEY_POWER` as a convergence primitive: power-on is `wake()` + readiness probe,
power-off is `sleep()` recorded as `Inferred`. A device whose power is `Unknown` and
that has no WoL is **quarantined**, not toggled.

### 6.3 Healer

`Wake → reconnect (capped backoff, max 60 s, ≤25 % jitter) → CEC (if compiled and
present) → Quarantine`, with a per-device counter keyed by `TvError` class. The
bounded backoff is the whole point: without it one AP reboot becomes a fleet-wide
reconnect storm.

### 6.4 Audit log

Append-only JSONL, local file, one line per action:

```json
{"ts":"2026-10-19T20:31:02Z","device_id":"…","command_id":"…","desired":"On/Hdmi2","action":"wake+select","result":"ok","confidence_after":"Inferred"}
```

Readable by a technician without a Rust toolchain. This is a product requirement.

## 7. Phases

All eight phases landed, in order, each with its exit criterion checked in CI or in a
test: audit → correctness slice → core + mock → dual-protocol conformance → Tizen
isolation (quirks **consumed**, not just loaded) → agent (providers, `decide()`,
healer, audit, 24-hour/50-device simulation) → fleet + HIL + CEC + CLI → hardening
(MSRV, `cargo-deny`, `forbid(unsafe_code)`, publication).

The phase table that tracked them was deleted on purpose: a checklist that is all
ticks is history, and history belongs in `git log` and the CHANGELOG. What remains
below is the part worth re-reading — the evidence that the definition of done was
actually met.

## 8. Verification record (v0.1 → v0.2.0)

Status as of `0.1.0` (published 2026-10-20). "Met" means a gate in CI or a test that
would fail if the claim stopped being true.

| # | Item | Status | Where it is checked |
|---|---|---|---|
| 1 | `tv-rs-core` builds `--no-default-features`, no tokio in the graph, docs complete | **met** | CI `core (tokio-free + mock)` and `per-crate features` jobs |
| 2 | Roku + webOS pass conformance on mocks **and** on one real device each, with the HIL result committed as a report | **mock half met, hardware half blocked** | `conformance` feature passes on the facade's three clients; `docs/hil-results.md` is empty because no panel answered SSDP |
| 3 | Tizen passes conformance on mocks and is marked **experimental** in its README | **met** | `crates/tizen/README.md` opens with "Status: experimental"; conformance passes |
| 4 | The agent survives the 50-device/24-hour simulation | **met** | `crates/agent/tests/fleet.rs::twenty_four_simulated_hours_with_three_blips` |
| 5 | `tv-rs-cli status` renders the confidence table; the audit log is readable by a non-Rust user | **met** | `status` prints `CONFIDENCE` (`high`/`inferred`); the audit log is JSONL with `ts`, `device`, `decision`, `result` |
| 6 | `NON_GOALS.md`, `SECURITY.md`, `INTEGRATION.md` exist and are linked from the README | **met** | all three at the repository root, each linked from `README.md` |
| 7 | `#![forbid(unsafe_code)]` on every crate except `tv-rs-cec` | **met** (stronger: including `tv-rs-cec`) | every `src/lib.rs` |
| 8 | The error taxonomy is covered by an exhaustiveness test | **met** | `crates/core/src/error.rs::remedy_matches_the_exhaustive_match` |

The only unmet item is the one that cannot be met from this repository: real hardware.
`cargo xtask hw-test --write-doc` fills `docs/hil-results.md` the moment a television
answers, and the two conformance runs it enables are the remaining gate.

## 9. Risk register

| # | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| 1 | Samsung disables a port on a new firmware generation | medium | high | port ladder + `quirks/`; anticipate, don't discover |
| 2 | LG changes pairing and freezes existing keys | medium | medium | `Auth(PairingRequired)` is first-class; re-provision flow tested in Phase 4 |
| 3 | `async-trait` allocation hurts fleet throughput | low | low | measured in the Phase 4 load test before anyone argues about it |
| 4 | Roku TV returns 200 without launching | high | medium | `Inferred`, verified via `/query/active-app` |
| 5 | AP isolation kills discovery on a real LAN | high | medium | static address + MAC manifest is a first-class path, not a workaround |
| 6 | Scope creeps into CMS/scheduler | high | high | `NON_GOALS.md`, checked in the PR template |

## 10. Decisions that were open, now closed

| Decision | Resolution | Why |
|---|---|---|
| License | **MIT only** | The dual-licence question had to be settled before the first publish and it is now published at `0.1.0`/`0.2.0`; changing it later is a governance change, not a release step. |
| Crate prefix | **`tv-rs-*`** (fourteen crates) | Short names were free, but the registry is immutable after the first upload; the prefix is now permanent. |
| `VolumeControl` in core | **Adopted** | All three consumer vendors expose volume; without the trait every consumer re-learns the vendor branch. Tizen and Roku still refuse *absolute* volume through the capability set, which is the honest version of the same decision. |
