# Phase 0 — audit and reconciliation

Phase 0 of the [device-model spec](00-device-model.md). This is a map, not a change:
for every load-bearing public item that exists today, where it goes, and what
happens to the code around it.

Measured on 2026-10-19 against `main` (`8` published-facing crates under `crates/`,
`xtask`, 136 tests, MSRV `1.75`, MIT, `#![deny(missing_docs)]` enforced from
`[workspace.lints.rust]`, no `unsafe` anywhere).

**Headline:** the playbook assumed greenfield. It is not. Roughly 70 % of the code —
every parser, every payload builder, every protocol test — survives this migration
untouched. What moves is the boundary: types that today live in whichever crate
first needed them (a `PowerState` in `tv-rs-power`, key enums in each backend) move
into `tv-rs-core`, and every backend gains a trait implementation.

---

## 1. Crate map

| Today | Spec | Verdict |
|---|---|---|
| `tv-rs` (facade, `TvClient` enum) | `tv-rs` | **keep name and enum**; additionally implements `DeviceHandle` |
| `tv-rs-discovery` | `tv-rs-discovery` | keep; change the return type to the core `DeviceDescriptor` |
| `tv-rs-power` | `tv-rs-power` | keep the WoL sender and probe; `PowerState` moves to core |
| `tv-rs-transport` | `tv-rs-transport` | keep; drop `TlsPolicy::AcceptInvalid` (pinning replaces it) |
| `tv-rs-roku` | `tv-rs-roku` | keep; implement capabilities, map errors |
| `tv-rs-webos` | `tv-rs-webos` | keep; implement capabilities, map errors |
| `tv-rs-tizen` | `tv-rs-tizen` | keep; implement capabilities, map errors |
| `tv-rs-media` | `tv-rs-media` | keep unchanged — it is not part of the device model |
| `tv-rs-cli` | `tv-rs-cli` | keep; add `status` (confidence table) and `ensure` |
| — | `tv-rs-core` | **new**: traits, `DeviceHandle`, `CommandId`, `ObservedState`, taxonomy, mock |
| — | `tv-rs-credentials` | **new**: one store for Tizen tokens, webOS `client-key`, cert pins |
| — | `tv-rs-agent` | **new**: Reconciler, Healer, providers, audit log |
| — | `tv-rs-cec` | **new, optional**: no-op when the feature is off |

## 2. Symbol disposition

### 2.1 Moves into `tv-rs-core`

| Symbol | Today | Disposition |
|---|---|---|
| `PowerState` | `tv-rs-power::types` | move; power crate re-exports nothing (callers import from core) |
| `MacAddress`, `MagicPacket` | `tv-rs-power` | **stay** — link-layer detail, not device-model vocabulary |
| `TizenRemoteKey` (22 keys) | `tv-rs-tizen` | becomes `Key` in core **plus** a vendor mapping table in the backend |
| `RokuKey` (22 keys) | `tv-rs-roku` | same: core `Key`, backend maps to ECP strings |
| `SsapUri` | `tv-rs-webos` | **stays** — it is the webOS wire vocabulary, not a device concept |
| `DiscoveredTv` | `tv-rs::types` | becomes `DeviceDescriptor` (adds `firmware`, stable `DeviceId`) |
| `TvVendor` | `tv-rs::types` | becomes `DeviceOs` in core (`Tizen`, `WebOs`, `Roku`, `Unknown`) |
| `TvError` | `tv-rs::error` | replaced by the six-variant taxonomy in §4.5 of the spec |
| `Unsupported` (transport) | `tv-rs-transport` | becomes `TvError::Unsupported(Capability)`; the transport keeps it only for the TLS/feature cases |

### 2.2 Stays where it is

| Symbol | Disposition |
|---|---|
| `SsdpScanner`, `build_msearch`, `parse_msearch_response`, `parse_device_descriptor`, `fetch_descriptor` | keep; wrap output into `DeviceDescriptor` |
| `WolSender`, `build_magic_packet`, `poll_power_state` | keep; `PowerControl::wake()` calls `WolSender` |
| `CertFingerprint`, `TlsPolicy::{Strict, Pinned, TrustOnFirstUse}`, `connector` | keep as-is; backends construct the policy |
| `RokuClient` + XML parsers, `WebosClient`/`WebosSession` + SSAP payloads, `TizenClient`/`TizenSession`/`TizenConnectionPool` + channel payloads | keep every parser and payload builder; add a trait impl per backend |
| `RangeError`, `parse_range`, `router`, `serve`, `MediaPlaylist` | untouched |
| CLI argument types | untouched, extended with new subcommands |

### 2.3 Merges into `tv-rs-credentials`

Three stores with the same required properties (atomic write, quarantine a corrupt
file, never clobber a good value with an empty one):

| Today | Size | Merge |
|---|---|---|
| `SamsungTokenStore` (IP-keyed JSON, `.json.corrupt` quarantine, no-clobber upsert) | `tv-rs-tizen/src/token_store.rs` | becomes `FileStore` backend, `CredentialKind::PairingToken` |
| `save_client_key` / `load_client_key` (one key, one file) | `tv-rs-webos/src/client.rs` | becomes `CredentialKind::ClientKey` |
| certificate pins (hex, caller-persisted today) | `tv-rs-transport` | becomes `CredentialKind::CertificatePin` |

The trait is `CredentialStore { get, put, remove, list }` keyed by
`(DeviceId, CredentialKind)`. The file backend inherits the Tizen store's semantics,
which are the correct ones.

### 2.4 Removed

| Symbol | Why |
|---|---|
| `TlsPolicy::AcceptInvalid` (and the `native-tls` accept-invalid branch) | `Pinned`/`TrustOnFirstUse` cover the self-signed-TV case; a policy that validates nothing should not ship in a published crate. If a debug escape hatch is wanted, it belongs in `xtask`, not in the library |
| `RokuAuthProvider` | already removed — Roku ECP authenticates nothing |
| `TvError`'s per-vendor variants (`Discovery`, `Power`, `Tizen`, `Webos`, `Roku`) as top-level routing | replaced by `Transport`/`Protocol`/`Auth`/… ; the crate errors remain as detail and convert at the boundary |

## 3. What has to be written from nothing

| Missing today | Phase |
|---|---|
| `CommandId`, dedup contract | 1 |
| `Confidence`, `ObservedField`, `ObservedState`, `DecayPolicy` | 1 |
| `DeviceHandle`, `CapabilitySet`, `Capability` | 1 |
| Mock backend with scripted failures | 1 |
| Shared `Key` / `Input` / `AppRef` vocabulary | 1 |
| Error taxonomy + exhaustiveness test | 1 |
| Conformance suite | 2 |
| `quirks/default.toml` + loader | 3 |
| `DesiredStateProvider`, `Reconciler`, `Healer`, audit log | 4 |
| `xtask hw-test`, fleet load harness | 5 |
| `NON_GOALS.md`, `INTEGRATION.md`, `QUIRKS.md` | 6 (NON_GOALS now) |

## 4. Conflicts with decisions already made in this repo

| # | Conflict | Resolution in the spec | Cost if reversed tomorrow |
|---|---|---|---|
| 1 | Playbook assumes greenfield | Phases rewritten as "wrap existing backends", not "write backends" | none — this is the cheaper path |
| 2 | Playbook: accept-invalid TLS behind a flag | pinning already shipped; `AcceptInvalid` removed | deleting a public variant after publish is breaking |
| 3 | Playbook crate names `tv-*` | `tv-rs-*` prefix kept; `tv-cli` is taken on crates.io (verified), `tv` too | renaming after the first publish is impossible |
| 4 | Playbook: dual MIT/Apache | MIT unchanged, flagged as an open decision before publish | relicensing needs every contributor's consent |
| 5 | `tokio::time::Instant` in `ObservedField` | `std::time::Instant` | none; it is the same type semantically |
| 6 | Enum (`TvClient`) vs dyn (`DeviceHandle`) | **both**: enum for one TV, trait objects for a fleet | removing either later is a breaking change |
| 7 | `PowerState` in `tv-rs-power`, keys per backend | move to core; backends map | moving later breaks every `match` |
| 8 | No `VolumeControl` in the playbook's trait list | added, because three vendors support it and it is the most common operation | adding a *trait* is non-breaking; adding it late means consumers wrote the branch |

## 5. Migration order (derived from the audit)

1. `tv-rs-core`: types and traits only, no behaviour. Backends compile against it
   with `todo!()`-free stubs — every method that cannot work yet returns
   `Unsupported`, which is legal today and legal forever.
2. Move `PowerState`/`DeviceDescriptor`/`DeviceId`, update the two call sites that
   exist (facade, CLI).
3. `TvClient` impls `DeviceHandle`; the CLI keeps working unchanged.
4. Backends implement capabilities one at a time (volume → app → key → input),
   each with conformance tests, each independently mergeable.
5. Only then add `CommandId` to the mutating methods — it touches every backend, so
   it goes after the traits are stable, not before.
6. `tv-rs-credentials` last among the refactors: it needs `DeviceId` to key on.

## 6. Compatibility statement

Nothing here is published yet (`crates.io` returns 404 for every workspace crate),
so this migration is free **today** and expensive after the first release. Two
consequences:

- the breaking changes in this document should land **before** the first `0.1.0`
  publish, or be batched into a `0.2.0` with a changelog that says plainly that the
  device model changed;
- the protocol crates' public API (parsers, clients) should not be renamed in the
  same wave — the break should be one boundary, not two.
