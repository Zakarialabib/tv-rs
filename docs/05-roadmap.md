# Where this goes next

Proposals, not commitments. Each row names the evidence behind it — a line of code
in an app that needs it, a behaviour a vendor documents, or a gap this workspace
already admits — and what it would cost.

Ordered by *value per unit of work*, which is why the boring ones are first.

## Where these stand (2026-10-20)

Checked against `main` the day `0.1.0` was published. "Landed" means a test or a gate
would fail if the claim stopped being true; the proposal text below is left as it was
written, so its reasoning is still readable.

| | Proposal | Status |
|---|---|---|
| P0 | facade volume / mute / input / app list | **partly open** — `open_url()` landed; volume, mute, input and `apps()` exist only behind the vendor accessors |
| P1 | errors that tell an app what to do | **landed in `tv-rs-core`** — six-variant `TvError` + `Remedy` routing; Roku's 403 already names the *Control by mobile apps* setting. The facade still returns vendor errors verbatim, so an app matching on `Remedy` must go through `DeviceHandle` |
| P2 | `tv-rs doctor` | **open** |
| P3 | fleet operations | **landed** — `tv-rs-agent`: providers, pure `decide()`, healer ladder, JSONL audit, 24-hour/50-device simulation |
| P4 | device identity | **landed** — `DeviceId`, `DeviceDescriptor`, `DeviceOs` in `tv-rs-core`; `DeviceHandle::id()` |
| P5 | event subscriptions | **open** — the agent polls, which is right at 60 s and wrong for an instant UI |
| P6 | one credential store | **open** — tokens and `client-key`s are still per-crate |
| P7 | panel control | **partly landed** — HDMI-CEC (`tv-rs-cec`) covers power and active source; no brightness/colour, which is vendor-specific everywhere |
| P8 | DLNA server | **open here** — `tv-rs-media` serves `206`/HLS only |
| P9 | more vendors | **open** — Android TV, Cast and AirPlay are each a separate trust model and crate |

Two proposals in P0 and one in P1 are worth revisiting in that light: the facade's
surface is now the *portable* subset, and everything vendor-shaped is deliberate.

---

## P0 — Finish the facade's vocabulary (small)

Today the facade can power off, launch an app and report power state. That is
enough to prove the abstraction and not enough to drive a TV panel: every app
immediately wants volume, mute and input source.

| Add | How | Why it is cheap |
|---|---|---|
| `TvClient::set_volume(u8)`, `volume_up()`, `volume_down()` | webOS has `set_volume`; Tizen has `KEY_VOLUP`/`KEY_VOLDOWN`; Roku has `VolumeUp`/`VolumeDown` keys | the per-vendor code exists, only the dispatch is missing — except Roku, which has keys but no absolute volume |
| `TvClient::set_mute(bool)` | webOS `set_mute`; Tizen `KEY_MUTE` (a toggle, so `set_mute(true)` twice unmutes — track state or expose `toggle_mute`); Roku `VolumeMute` | two vendors toggle rather than set: the API must say so |
| `TvClient::input_source()` / `set_input_source(id)` | webOS `ssap://tv/switchInput` + `ssap://tv/getExternalInputList` (two new `SsapUri` variants); Tizen: no public input API in the remote channel → `Unsupported`; Roku: keypress navigation only | honest `Unsupported` is a feature, not a gap |
| `TvClient::apps()` | Roku `/query/apps` (already parsed); webOS `ssap://com.webos.applicationManager/listApps`; Tizen REST `GET /api/v2/applications` on port 8001 — a new endpoint | Roku is free; the other two are one request each |

**Evidence:** an app with a display grid needs all four on every tile; today it
would have to reach through `as_webos()`/`as_tizen()`/`as_roku()` and re-implement
the vendor branch that `TvClient` exists to absorb.

## P1 — Errors that tell an app what to do (small, high leverage)

Three failures are currently indistinguishable from "unreachable", and each has a
different remedy:

| Failure | Real cause | Needed variant |
|---|---|---|
| Roku refuses ECP writes | Roku OS 14.1+ requires **Settings → System → Advanced → *Control by mobile apps*** to be enabled for keypress/launch | `TvError::NotPermitted { remedy }` |
| webOS prompts on every connect | no `client-key` yet, or it was revoked on the TV (`pair()` exists but nothing forces a caller to notice) | `TvError::NeedsPairing` |
| TLS handshake fails on a pinned TV | the TV regenerated its certificate | `TvError::CertificateMismatch { expected, observed }` |

Without these, a UI shows "TV unreachable" for a setting the user can flip in ten
seconds. **Evidence:** the Roku 14.1 restriction is documented on Roku's ECP page;
webOS pairing is in `docs/01-playbook.md` §4; pinning shipped in this workspace
with a mismatch message that is currently only a string inside `rustls::Error`.

## P2 — `tv-rs doctor` (small, disproportionate payoff)

A CLI subcommand that takes an IP or scans, and prints: vendor, model, firmware,
control port, whether the token/client-key exists, the certificate fingerprint, and
what the device answers to each capability probe.

Why it earns its keep:
- it is the fastest way to answer "why won't this TV respond?" for a user, a bug
  report or a support ticket;
- it is the honest replacement for hardware tests we cannot run in CI — a user can
  paste its output;
- it costs almost nothing: every probe already exists in the crates.

> P3 and P5 below are specified in detail in
> [`spec/00-device-model.md`](spec/00-device-model.md).

## P3 — Fleet operations (medium)

The per-device API is right for a library; every consumer immediately writes the
same loop. A `TvFleet` that fans out with bounded concurrency and aggregates
per-device results (`Vec<(Address, Result<…>)>`, never `?` on the first failure)
would remove the most-copied code in the ecosystem.

Also belongs here: a **last-known-address cache**, because discovery takes seconds
and a fleet poll takes none if you already know where the TVs were.

## P4 — Device identity (small-medium)

`DiscoveredTv` carries vendor, address and label. It does not carry a stable id —
and a fleet needs one that survives a DHCP lease change.

- `DeviceId` from the UPnP `UDN` when present, else the Roku `device-id`, else MAC;
- MAC resolution through the ARP table (also unlocks Wake-on-LAN for every device,
  because WoL needs a MAC and discovery does not return one);
- typed `model_name`, `model_number`, `firmware` from the descriptor instead of
  leaving consumers to grep XML.

**Evidence:** every display record in a signage app eventually grows `vendor` and
`mac` columns; both are derivable at discovery time and are re-derived by hand
today.

## P5 — Event subscriptions (medium, biggest conceptual gap)

`tv-rs` is request/response only. webOS supports `ssap://…/subscribe` and pushes
state (current app, input, volume, power). Roku has `/query/active-app` to poll.
Tizen has neither.

A `TvEventStream` (`async fn next(&mut self) -> Option<TvEvent>`) with vendors that
support push emitting real events and others emitting polled snapshots would turn
"is the display still playing?" from a guess into an answer. This is the single
feature most likely to make an app delete its own health-check code.

## P6 — One credential store (small)

Three almost-identical stores are about to exist: Tizen's `SamsungTokenStore`,
webOS's `client-key` file and the certificate pin. Each needs the same properties —
atomic write, quarantine on corrupt input, never clobber a good value with an empty
one. One `CredentialStore` (JSON, platform-appropriate path, per-device entries)
would carry all three, and document the security posture once.

## P7 — Panel control (small)

Power is not screen: signage wants the panel off with the box awake on a schedule.
- webOS: `ssap://com.webos.service.tvpower/power/turnOffScreen` / `turnOnScreen`
- Tizen: no public API; `KEY_POWER` is a device toggle, not a panel toggle
- Roku: `DisplayOff` exists; no equivalent "screen on" key

So: `TvClient::set_screen(bool)` returning `Unsupported` where the vendor cannot do
it. Do not fake it with power.

## P8 — Media: become a DLNA server, not just an HTTP server (large)

`tv-rs-media` serves `206` and HLS. A TV cannot find it. Adding SSDP advertisement
plus a minimal `ContentDirectory` browse would let a TV pull content from this crate
directly, with no control point in between. Large, and only worth it if the goal is
"a TV and nothing else".

## P9 — More vendors, as separate crates (large)

| Platform | Precedent | Note |
|---|---|---|
| Android TV / Fire TV | `adb_client`, `radb` on crates.io | ADB over TCP 5555; requires developer mode on the device; a genuinely different trust model |
| Google Cast | `pychromecast` | mDNS discovery + TLS 8009 + protobuf; needs mDNS first |
| AirPlay | `pyatv` | HAP-style pairing; heavy |
| Philips JointSpace / Vizio SmartCast / Hisense | vendor REST APIs | same shape as Roku: cheap, low value per vendor |

Each belongs in its own crate: they share nothing with the current five except the
willingness to return typed errors.

---

## Project-level improvements (no new features)

1. **MSRV in CI.** `rust-version = "1.75"` is declared and never tested. One job
   with `cargo +1.75 check --workspace` either proves it or corrects it.
2. **`cargo deny` / `cargo audit` job.** Publishing a network-facing crate without
   an advisory gate is a promise nobody checked.
3. **docs.rs build check.** `cargo doc --all-features` in CI catches the
   feature-gated doc breakage that only shows up after a release.
4. **A hardware matrix file.** `docs/hil-results.md`: vendor, model, year, firmware,
   what passed. Replaces "verified on a real TV" with a date and a serial number.
5. **A capability table per vendor in rustdoc.** Today "webOS does X, Tizen does
   not" is prose in the playbook; it should be a table generated from the
   `Unsupported` returns, so the docs cannot drift from the code.

---

## What I would do next, in order

1. **P0 + P1** in one PR — the facade vocabulary and the three error variants. Both
   are small, both are additive, and together they make the crate usable by an app
   without reaching past the facade.
2. **P2 (`doctor`)** — it converts every future "it doesn't work" into a pasteable
   report, which matters more while nothing has been verified on hardware.
3. **P4 identity + ARP** — unlocks Wake-on-LAN for devices that were discovered but
   never paired, and stops consumers inventing their own device keys.
4. **P5 subscriptions** — the largest remaining conceptual gap, and the one that
   changes what consumers can build.
