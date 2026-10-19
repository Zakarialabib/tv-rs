# Signage adoption — analysis and extension plan

How the `Simple-Signage` (SignageMaster) desktop app absorbs these crates. This
document is the *consumer* view: what the platform already runs, what the crate
actually adds, and the prompts that turn that into a plan and then into code.

The signage repository is external. Every path below is relative to it
(`C:\laragon\www\Simple-Signage`), and every claim about it was read from source
on 2026-10-19, not inferred.

## 1. What the platform already runs

| Capability | Where | Evidence |
|---|---|---|
| SSDP discovery **and** advertising (NOTIFY/byebye + M-SEARCH responder) | `src-tauri/crates/plugin-dlna/src/{discover,advertise}.rs` | 8 + 5 tests |
| DLNA MediaRenderer control (AVTransport, description.xml, SOAP Browse) | `src-tauri/src/network/dlna_media_server.rs` | 11 tests, incl. 206/416 ranges |
| Samsung discovery (ST `urn:samsung-com:device:RemoteControlReceiver:1`) | `plugin-samsung-remote/src/discovery.rs` | 8 tests |
| Samsung pairing + token store (quarantine, no-clobber) | `plugin-samsung-remote/src/{remote,tokens}.rs` | 9 + 6 tests |
| Samsung keys, app presets, 8001→8002 fallback, **connection pool with eviction** | `plugin-samsung-remote/src/remote.rs` | within the 9 |
| Samsung IPC surface (`samsung_*`, 7 commands, Zod, registry) | `plugin-samsung-remote/src/commands.rs`, `src/core/ipc/samsung*.ts` | 3 Rust + contract gates |
| mDNS for own players, Miracast | `network/lan_display_discovery.rs`, `plugin-display/src/miracast*.rs` | — |
| Wake-on-LAN | **setting only** (`wake_on_lan: bool` in `plugin-hardware`) — no magic-packet sender found | grep: 0 senders |

**Conclusion.** Discovery, DLNA, media serving and Samsung control are *already
owned* by the platform and are deeper than this crate in places. The crate's
value is therefore **not** those four; it is the vendors the platform does not
speak and the parts of Samsung it was missing.

## 2. Gap analysis

| Capability | Platform | Crate | Verdict |
|---|---|---|---|
| LG webOS (SSAP) | absent (`ssap://` = 0 hits) | `tv-rs-webos` (1,014 loc) | **adopt** — new plugin |
| Roku ECP | absent | `tv-rs-roku` (674 loc) | **adopt** — new plugin |
| Wake-on-LAN sender + liveness probe | setting, no sender | `tv-rs-power` (457 loc) | **adopt** — wire behind the existing toggle |
| Samsung discovery / token store / presets / pool | present (their code) | now present in `tv-rs-tizen` (35 tests) | **retire their internals**, keep the IPC surface |
| Vendor classification for DLNA hits | raw `manufacturer` string | `identify_vendor()` | **adopt** — cheap, improves the display grid |
| SSDP/DLNA advertise + SOAP Browse | present, tested | `tv-rs-media` has no SOAP (deliberate) | **keep theirs** — the crate is not a DLNA control point |
| TLS backend | rustls only, `native-tls` banned | `rustls` default, `native-tls` opt-in | **resolved** — crate now satisfies the policy |

## 3. Migration phases

| Phase | Work | Exit criterion |
|---|---|---|
| **A — audit** | inventory the display/TV surface, no code | a table of every TV-touching command, its plugin, its tests, and its gap |
| **B — plan** | ADR: which plugin gains which vendor, IPC shape, migration order | approved plan, no code |
| **C — LG + Roku** | two new plugins against `tv-rs-webos` / `tv-rs-roku`, mirroring the `samsung_*` contract | contract tests + Zod coverage green, no `unwrap` in command bodies |
| **D — power** | `tv-rs-power` behind the existing `wake_on_lan` toggle; probe feeds display health | WoL actually emits; `Standby` vs `Off` surfaces in the UI |
| **E — Samsung swap** | replace `plugin-samsung-remote` internals with `tv-rs-tizen`, keeping all 7 commands | parity matrix signed off; their 18 tests either still pass or are re-homed |
| **F — cleanup** | delete the duplicated modules, `check:wiring` / `check:manifests` green | no dead crate, no orphaned manifest entry |

Phases C and D are independent and can run in parallel. E is last because it
touches a working, tested surface.

## 4. Prompts

Each prompt is for an agent working **inside the signage repository**. They are
ordered: A and B produce artefacts, C–E produce code.

### P1 — Audit the existing TV/display surface

> Analyse this repository's television and display control surface without
> changing code. Produce one table with a row per capability and these columns:
> capability, implementing file(s), IPC command name(s), test count, gate that
> covers it, and whether it is desktop-gated.
>
> Cover at minimum: SSDP discovery and advertise, DLNA media server and SOAP
> Browse, Samsung remote (discovery, pairing, token store, keys, app launch,
> connection pool), mDNS player discovery, Miracast, display hardware controls
> (`plugin-display/src/hardware/commands.rs`), and any Wake-on-LAN sender (if
> none exists, say so explicitly rather than listing the `wake_on_lan` *setting*).
>
> Rules: cite files by path; do not infer a capability from a config flag; a
> capability with no test is reported as "untested", not omitted. End with the
> three largest parity risks of moving any of it onto an external crate.
>
> Done when the table is complete enough that a reviewer can name every place a
> TV is contacted without opening the repo.

### P2 — Write the extension plan (no code)

> Using the audit from P1, write an ADR that decides how LG webOS, Roku ECP and
> Wake-on-LAN enter the platform.
>
> Required content:
> - **Ownership**: which module/plugin owns each vendor, and why not one generic
>   "tv" plugin (the existing `CameraProvider`-style boundary is the precedent to
>   follow or reject — say which).
> - **IPC shape**: the exact command names for LG and Roku, chosen to mirror the
>   existing `samsung_*` seven (`discover / pair / send_key / launch_app /
>   disconnect / list_paired / forget_paired`). Where a vendor cannot support a
>   verb (Roku has no pairing), say so and define the 501-equivalent behaviour.
> - **Contract preservation**: the Samsung surface may not change in this phase.
>   List the Zod schemas, `CommandRegistry` entries and manifest capabilities
>   that must stay byte-identical.
> - **Data**: where LG `client-key`s live (they are secrets — next to the Samsung
>   token file, not in SQLite), and what a display record gains (vendor, model,
>   MAC for WoL).
> - **Order and rollback**: which phase ships first, and what "revert" means for
>   each.
>
> Constraints: desktop-gated like the Samsung wave; no new frontend plugin
> directory if Zod can live in `src/core/ipc/`; every new command needs a test and
> a Zod schema in the same PR.
>
> Done when the ADR names every file to be created or touched.

### P3 — LG webOS plugin

> Implement the LG plugin against `tv-rs-webos`, following the ADR from P2 and
> mirroring `plugin-samsung-remote`'s layout (`src/lib.rs` with `handler()`,
> `commands.rs`, error type, manifest, `src/core/ipc/lg*.ts`, Zod in
> `src/core/ipc/`).
>
> - Pairing: `WebosClient::new(host).with_device_name("SignageMaster").connect()`
>   → persist `session.client_key()` beside the Samsung token file; reconnect
>   with `.with_client_key(saved)` so the TV never re-prompts.
> - Commands map 1:1 onto the Samsung verbs where they exist: `discover`
>   (`discover_samsung`-equivalent for LG means SSDP + `identify_vendor()`
>   filtering to `TvVendor::Lg`), `pair`, key presses (webOS has no remote-key
>   API — define the closest equivalent or return the documented
>   unsupported error), `launch_app`, `disconnect`, `list_paired`, `forget_paired`.
> - Use the typed helpers (`set_volume`, `set_mute`, `power_off`, `foreground_app`)
>   rather than hand-built `ssap://` strings.
> - Errors: map `WebosError::Unsupported` to the same "vendor cannot do that"
>   signal the Samsung plugin uses; never `unwrap()` a command body.
>
> Done when: contract tests, Zod coverage and the manifest gate pass, and a live
> LG TV can be paired once and controlled from the UI afterwards without
> re-prompting.

### P4 — Roku plugin

> Implement the Roku plugin against `tv-rs-roku` with the same contract shape.
>
> - ECP is unauthenticated: `pair` is a no-op that records the device, and
>   `list_paired` must not imply a credential exists.
> - `launch_app` resolves a human name via `query_app_id(name)` before launching.
> - `send_key` maps to `RokuKey` variants; reject unknown key names with the
>   same typed error the Samsung path uses.
> - Discovery: filter SSDP results through `identify_vendor()` to `TvVendor::Roku`.
>
> Done when: the plugin passes the same gates as P3 and a Roku can be launched by
> app name from the UI.

### P5 — Move Samsung onto the crate

> Replace `plugin-samsung-remote`'s internals with `tv-rs-tizen` while keeping the
> IPC surface byte-identical.
>
> - Keep the seven command names, their argument names (snake_case on the wire),
>   their Zod schemas and the manifest capability list.
> - Replace `discovery.rs` with `tv_rs_tizen::discover_samsung`; replace
>   `tokens.rs` with `tv_rs_tizen::SamsungTokenStore` (same quarantine and
>   no-clobber semantics — verify against the old tests); replace the connection
>   pool with `TvConnectionPool` (one reopen on a dead channel).
> - Preserve the app presets: `resolve_app_id` covers netflix/youtube/prime; any
>   additional preset the platform ships must move into the crate in the same PR
>   or stay in the plugin — do not silently drop one.
> - The old tests are the spec: a test that cannot be re-homed onto the crate is a
>   parity gap, not a deleted test.
>
> Done when: the parity matrix from P2 is fully green, `check:wiring` and
> `check:manifests` pass, and no module is left imported-but-unused.

### P6 — Parity and cleanup gate

> Delete the code P5 replaced and prove nothing was lost.
>
> - Run the full gate set: `cargo check`, the contract/wiring/manifest checkers,
>   the frontend contract tests and the doc-count checker.
> - Produce a before/after table: every removed file, the crate symbol that
>   replaced it, and the test that now covers the behaviour.
> - Any behaviour that could not move (for example a connection-pool detail the
>   crate models differently) is documented as an explicit, reviewed difference —
>   not quietly deleted.
>
> Done when the table has no empty "replaced by" cell and the gates are green on
> a clean tree.

## 5. Risks and non-goals

- **Feature unification.** Two plugins depending on `tv-rs-*` unify features
  across the workspace; if the platform ever needs `native-tls` for another crate,
  the TLS backend becomes a workspace-wide decision. The crate makes the conflict
  a compile error rather than a silent double-stack.
- **Secrets on disk.** LG `client-key` and Samsung tokens are credentials; they
  belong in the file store the platform already uses, not in the shared SQLite
  database or a log line.
- **Not adopting:** SSDP advertising, SOAP Browse/ContentDirectory, the DLNA
  media server. The crate deliberately does not implement them; `plugin-dlna`
  keeps ownership.
- **Not in scope:** mobile surfaces. The Samsung wave is desktop-gated and these
  follow it.
