# Android TV / Google TV remote — v2 spike doc (plan item 2.4)

**Status:** research + probe plan (headless). No panel available in this workspace.
**Spike branch:** `spec/atr-v2-spike`. **Depends on:** `PR #86` merged into `main`
(`5113de2`) for the G18 SubscriptionHandle plumbing.

---

## 1. Why Android TV / Google TV

The peer landscape (03-matrix.md, researched 2026-10-23) found a second door into
the Android TV trust boundary beside ADB:

- **ADB's model** = developer mode + per-TV pairing. It needs a menu path and a
  PIN, and it's the "privileged fallback" (Reboot + arbitrary intents).
- **The Android TV Remote protocol v2** = the protocol the Google TV remote app
  speaks: mTLS on fixed port **6467** (Polo pairing handshake), protobuf
  commands on fixed port **6466**, device name + MAC parseable from the server
  certificate on the pairing port. No developer mode, no ADB.

Atvremote2 is the maintained Python reference implementation; the tv-rs
`tv-rs-androidtv` crate is a *wrap*, not a port, per the hardware-first rule.

---

## 2. Protocol facts (verified offline, no panel)

**Pairing — port 6467 (mTLS, "Polo" handshake).** The server presents a
self-signed certificate; the client pins it (and the client certificate is
unauthenticated — mutual-auth is NOT used). The pairing request carries
`client_name` + `service_name = "atvremote"`. After the client sends the
request the device responds with a 6-byte hex **pairing code** and the device's
cert (the client reads the cert from the TLS session). The client then:

1. pulls modulus/exponent from the client cert and the server cert,
2. computes `SHA-256(client_modulus || 0x0 || client_exponent || 0x0 ||
   server_modulus || 0x0 || server_exponent || pairing_code[2:])`,
3. sends `secret = hash` back; the device accepts only if the first byte of the
   hash equals `pairing_code[0:2]` hex,
4. marks pairing finished and opens the control port.

`name + MAC` are parsed out of the server certificate subject
(`CN=atvremote/<name>/<MAC>` for NVIDIA SHIELD, `dnQualifier=fugu/fugu/<name>`
for Nexus Player).

**Control — port 6466 (protobuf, TLS).** Frames are `RemoteMessage` with
`ProtoType` selecting the payload (e.g. `RemoteSetVolumeLevel`,
`RemoteStart`, `RemoteAppInfo`). A response is a `RemoteMessage` with
`ProtoType` = `Unknown` and a payload = the command's reply type.

**Command catalog:**

| Message | Key fields |
| --- | --- |
| `RemoteSetVolumeLevel` | `volume_max`, `volume_level`, `volume_muted` |
| `RemoteAppInfo` | `app_package` (e.g. `com.google.android.youtube.tv`) |
| `RemoteAppLinkLaunchRequest` | deep link, `uri` scheme form |
| `RemoteKeyCode` | `TvKeys.txt` enum (DPAD_*, BUTTON_*, MEDIA_*, TV_*, etc.) |
| `RemoteStart`, `RemoteStop`, `RemotePause` | player control |

**Reference client** (`tronikos/androidtvremote2`, main):
`androidtv_remote.py` (pairing + command paths), `pairing.py`
(`PairingProtocol` — 4-step Polo handshake), `polo_pb2.proto`
(`OuterMessage`, `RemoteMessage`), `base.py` (`ProtobufProtocol` framing),
`remotemessage.proto` (canonical wire shapes), `TvKeys.txt` (key catalog),
`demo.py` (session walkthrough), `pyproject.toml`.

---

## 3. The 9 probes (order per the brief; window A1/A2 per §2.2 of 06-plan.md)

Acronyms: **C**onnect, **P**air, **S**tate, **V**olume, **K**ey, **A**pp, **L**ink,
**E**rror, **R**econnect.

| # | Probe | Wire shape expected | Pass criterion |
| --- | --- | --- | --- |
| 1 | Connect + handshake | mTLS to 6467; `OuterMessage{protocol_version=2, status=OK, pairing_request{client_name, service_name=atvremote}}` | client pins server cert; pairing request accepted |
| 2 | Pairing code + secret | device replies with 6 hex code + cert; client answers `OuterMessage{protocol_version=2, status=OK, secret{secret=sha256}}` | pairing finished; no `InvalidAuth` |
| 3 | State read | `RemoteMessage{protocol_version=2, status=OK, message{request_type=GET_STATE}}` | `is_on`/`is_available`/`current_app` callbacks fire; `current_app` = known foreground app |
| 4 | Volume read | `RemoteMessage{protocol_version=2, status=OK, message{request_type=GET_VOLUME}}` | `volume_max`, `volume_level`, `volume_muted` populated |
| 5 | Volume round-trip | `RemoteMessage{protocol_version=2, status=OK, message{request_type=SET_VOLUME, set_volume_level{volume_level=...}}}` | read-back on 6466 shows the new level |
| 6 | Key press | `RemoteMessage{protocol_version=2, status=OK, message{request_type=KEY, key_code=...}}` | TV reports the key (or visibly responds on the panel) |
| 7 | App info / foreground | `RemoteMessage{protocol_version=2, status=OK, message{request_type=GET_APP_INFO}}` | `app_package` resolves to the running app |
| 8 | Deep link | `RemoteAppLinkLaunchRequest` with a `ssap://`-style URI | launched on the panel (logged by the app) |
| 9 | Error + reconnect | malformed command; disconnect; reconnect; phone-side sender (A2) | no panic, the facade retries once and reports the gate as failed |

**Not covered by this spike:** streaming a TV's content (that is the
`tv-rs-androidtv` crate after the verdict), and ADB-adjacent intents (covered by
A5/C1 in the host layer).

---

## 4. Verdict criteria (the spike's own exit)

- **Primary:** ATR v2 is accepted as the phone-side projection/control transport
  for Android TV / Google TV (no developer mode, no ADB).
- **Fallback:** ADB remains the privileged fallback (Reboot + arbitrary intents).
- **Wake ladder:** WOL/CEC wake for deep-sleep panels (A3), *after* the verdict.
- **Deliverable rows:** one probe row per gate in `08-hil-results.md`, plus the
  verdict written in this doc; `03-matrix.md` capability truth updated the day a
  crate is decided either way.

---

## 5. Open (pending the brief + a panel)

- The brief `androidtv-v2/references/atr-v2-brief.md` is not in this workspace
  (it lives in the companion app's `androidtv-v2/` tree). Collecting it is the
  remaining pre-probe step; the wire shapes above are consistent with the
  reference client and the 02-playbook.md shapes.
- The probe run itself requires a real Android TV panel in this workspace.
