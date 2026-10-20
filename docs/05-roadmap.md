# Closed proposals

This page is an **archive**. The P0–P9 proposals below were written before the device
model existed; every one of them has an outcome now, and live work lives in
[`06-plan.md`](06-plan.md).

It is kept for one reason: a proposal that was rejected is worth more as a recorded
"we considered this and here is why not" than as a thing someone re-proposes in six
months.

| | Proposal | Outcome |
|---|---|---|
| P0 | Facade volume / mute / input / app list | **Partly open.** `open_url()` landed. Volume and mute remain behind the vendor accessors by design: two vendors toggle rather than set, and a facade method that lies about that is worse than an accessor that tells the truth. Input switching has no public API on Tizen's remote channel at all. |
| P1 | Errors that tell an app what to do | **Landed in `tv-rs-core`**: six-variant `TvError` + `Remedy` routing (`Retry`, `BackOff`, `Reprovision`, `Wait`, `Disable`, `ApplyQuirk`). Roku's 403 already names the *Control by mobile apps* setting. The facade still returns vendor errors verbatim, so an app that wants remedy-based branching goes through `DeviceHandle`. |
| P2 | `tv-rs doctor` | **Open** — superseded in practice by `tv-rs-cli status` (power, confidence, capability set per device) and `cargo xtask hw-test`. A dedicated `doctor` would duplicate both. |
| P3 | Fleet operations | **Landed** as `tv-rs-agent`: providers (file/HTTP/MQTT), pure `decide()`, healer ladder, JSONL audit, `tick()`/`run()`, 24-hour/50-device simulation. |
| P4 | Device identity | **Landed**: `DeviceId`, `DeviceDescriptor`, `DeviceOs`, `DeviceHandle::id()`. |
| P5 | Event subscriptions | **Open.** The agent polls, which is right at 60 s and wrong for an instant UI. This is the largest conceptual gap left, and it is *not* on the near-term plan because nothing consumes it yet. |
| P6 | One credential store | **Open** by design: Tizen tokens and webOS `client-key`s stay in their crates because they are not the same credential and do not share a lifecycle. |
| P7 | Panel control | **Partly landed** — HDMI-CEC covers power and active source; brightness arrived with MDC (`tv-rs-mdc`), which is where a commercial panel exposes it. |
| P8 | DLNA server | **Landed differently**: `tv-rs-dlna` is the *transport* (SSDP, AVTransport, GENA, advertisement), and the media library stayed app-owned. `tv-rs-media` still only serves `206`/HLS. |
| P9 | More vendors | **Started**: Samsung MDC landed (`tv-rs-mdc`); Sony Bravia is the next one on the plan. Android TV, Cast and AirPlay remain separate trust models and separate crates. |

## What the archive teaches

Three of these proposals (P3, P4, P8) were resolved by *narrowing* them: the fleet
layer became a reconciler rather than an orchestrator, identity became a typed id
rather than a registry, and the DLNA server became a transport with the library left
outside. That pattern — ship the mechanism, refuse the policy — is why the workspace
still has no CMS in it.

Two were resolved by **measuring** instead of arguing: `async-trait` allocation
(risk 3 in the spec) never showed up in the 50-device simulation, and the quirks table
only became useful once a backend actually consulted it.
