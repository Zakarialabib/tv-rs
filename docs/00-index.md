# Documentation

Fourteen crates, one facade, one idea: speak each TV's own protocol and never hide
a failure behind a panic. These pages are the long version of that. This index
routes to everything that exists in `docs/` — nothing here, and nothing missing.

## Read in this order

| #   | Page                                                         | What it answers                                                                                                                                                                                                   |
| --- | ------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 0   | [`glossary.md`](glossary.md)                                 | **The map.** Every term, and the page or file that explains it. Start here if you only have five minutes.                                                                                                         |
| 1   | [`01-playbook.md`](01-playbook.md)                           | The protocol facts, numbered §1–§9: ports, handshakes, MDC frames, DLNA, media, and what each vendor exposes.                                                                                                     |
| 2   | [`architecture.md`](architecture.md)                         | The layers, the dependency rule, the module inventory per crate, the auth rule, and the playbook → crate map.                                                                                                     |
| 3   | [`03-matrix.md`](03-matrix.md)                               | State: what each crate implements today, the **capability truth** table, the registry versions, the verified test counts, and the honest gaps.                                                                    |
| 4   | [`02-external-refs.md`](02-external-refs.md)                 | Protocol references, prior art, and the crates this workspace learned from.                                                                                                                                       |
| 5   | [`04-rust-package-standard.md`](04-rust-package-standard.md) | The engineering standard the whole workspace follows — reusable outside this repo.                                                                                                                                |
| 6   | [`06-plan.md`](06-plan.md)                                   | **The live plan.** Open work only: the `0.3.0` publish, Sony, native CEC, DDC/CI, hardware, media caching, the next release. Landed work lives in the CHANGELOG.                                                  |
| —   | [`../INTEGRATION.md`](../INTEGRATION.md)                     | **Putting it in an application**: discovery, TLS pinning, quirks, a fleet, media, CEC, the pre-ship checklist.                                                                                                    |
| —   | [`07-adoption.md`](07-adoption.md)                           | The contract for adopting this workspace inside an application: what the app owns, what the crate owns, the no-duplication rules.                                                                                 |
| —   | [`hil-results.md`](hil-results.md)                           | What has actually run against a physical TV. Today: nothing.                                                                                                                                                      |
| —   | [`adr/`](adr/)                                               | [001 — crate publication](adr/001-crate-publication.md), [002 — workspace layout](adr/002-workspace-layout.md).                                                                                                   |
| —   | [`internal/`](internal/)                                     | Notes kept out of the public set: [deferred-auth](internal/deferred-auth.md) (why four deferred surfaces stay deferred), [capability-honesty](internal/capability-honesty.md) (where the capability table lives). |

## Conventions used here

- **Claims about code are checked against code.** If a page says a function
  exists, it exists in `crates/`. When the two disagree, the code is right and the
  page is a bug.
- **Deferred is a feature.** A capability that is not implemented returns a typed
  `Unsupported` error with a reason. No page promises something the code refuses
  to do.
- **Numbers are dated.** Test counts and the registry table in `03-matrix.md`
  carry the date they were measured; they rot, the source does not.
- **The plan is one page.** `06-plan.md` is the only plan; an item leaves a page
  when it lands. What landed is the [`CHANGELOG.md`](../CHANGELOG.md); a page
  that accumulates ticks is a second changelog.

## Where the code lives

| Path                                                | Contents                                                                                                        |
| --------------------------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| `crates/core`                                       | the device model: capability traits, the erased `DeviceHandle`, `CommandId`, observed state, the error taxonomy |
| `crates/tv-rs`                                      | facade: discovery orchestration, vendor identification, `TvClient`                                              |
| `crates/{discovery,power,tizen,webos,roku,mdc,cec}` | one wire protocol each (SSDP, Wake-on-LAN, Tizen, webOS, Roku ECP, Samsung MDC, HDMI-CEC)                       |
| `crates/local`                                      | the host's own display (EDID/mode probing)                                                                      |
| `crates/{transport,dlna,media}`                     | support: TLS/frame reader; DLNA renderer transport; `206`/HLS server                                            |
| `crates/app`                                        | the TV app producer: a supplied payload in, `.wgt` / webOS app directory / web bundle out                       |
| `crates/agent`                                      | reconciliation: desired state in, fleet converged, audited                                                      |
| `crates/cli`                                        | the reference consumer (`publish = false`)                                                                      |
| `xtask`                                             | workspace automation: `cargo xtask publish`                                                                     |
| `quirks/`                                           | the quirks table as data                                                                                        |
| `docs/`                                             | these pages                                                                                                     |
