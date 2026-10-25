# Protocol playbook

What each TV protocol actually does, at the level of detail you need to write or
debug a client: ports, handshakes, payload shapes, and the traps that cost an
afternoon. The sections are numbered (`§1`–`§9`) because
[`architecture.md`](architecture.md)'s playbook→crate map cites them.

This page is the reference the crates were built from. It is deliberately
protocol-only: no vendor app is described here, and nothing depends on one.

Source evidence: `crates/{discovery,power,tizen,webos,roku,mdc,cec,dlna,media}`,
[`02-external-refs.md`](02-external-refs.md) for where each protocol is
documented.

## §1 Discovery

Three mechanisms find a TV on a LAN. This workspace implements the first.

**SSDP** (Simple Service Discovery Protocol) — UDP multicast to
`239.255.255.250:1900`. Send an `M-SEARCH`; devices answer with `LOCATION`,
`SERVER`, `USN` and `ST` headers. `LOCATION` is an HTTP URL serving an XML
description with `<friendlyName>`, `<manufacturer>`, `<modelName>` and `<UDN>`.
No credentials are involved at any point.

```
M-SEARCH * HTTP/1.1
HOST: 239.255.255.250:1900
MAN: "ssdp:discover"
MX: 3
ST: ssdp:all
```

Traps:

- Header names are case-insensitive, and each line must be split once only — a
  `LOCATION` value contains colons.
- The same device answers several times (once per service). Deduplicate by
  `USN`, or by `LOCATION`.
- A firewall that eats multicast looks exactly like "no TVs here". Distinguish a
  bind failure from an empty sweep, or you will chase a phantom.

**mDNS / Bonjour** — UDP 5353, used by AirPlay and Cast (`_airplay._tcp.local`,
`_googlecast._tcp.local`). Not implemented here.

**ARP** — resolving an IP to a MAC so Wake-on-LAN can target a sleeping device.
The ARP probe itself is not implemented: a magic packet is addressed to a MAC, and the
facade takes one from the caller (`TvClient::with_mac`) or reads it off a Roku
(`/query/device-info` carries `<wifi-mac>` / `<ethernet-mac>`) rather than walking the
LAN's ARP table. Store the address once and reuse it — that is what
`DeviceDescriptor::mac` is for. Samsung and webOS have no verified MAC query here, so
`learn_mac()` refuses on them rather than guessing at an endpoint.

## §2 Power

**Wake-on-LAN** — a UDP "magic packet" to port 9: six `0xFF` bytes, then the
target MAC six bytes repeated sixteen times (102 bytes total). It is the only
reliable way to leave a hard-off state; every other approach assumes the TV is
awake.

Broadcast to `255.255.255.255` by default, but directed broadcasts
(`192.168.1.255:9`) survive routers that drop the all-ones address.

**Power-state probing** — a short TCP connect tells you more than any status
endpoint:

- connect succeeds → the application services are up (`On`);
- connection refused → the network interface answers but services are down
  (`Standby`, i.e. the TV is "off" but listening);
- timeout → nothing answered (`Off`);
- anything else → `Unknown`.

`Standby` is the state that matters for a control loop: WoL worked, the service
has not started yet. Retry, don't fail.

## §3 Samsung Tizen

**Remote channel** — a WebSocket at `/api/v2/channels/samsung.remote.control`:

- `ws://<ip>:8001` on legacy sets (plaintext),
- `wss://<ip>:8002` on 2018+ sets (TLS, self-signed certificate).

The name is passed as a query parameter, base64-encoded, and identifies the
client in the TV's on-screen prompt.

**Handshake**

```json
{ "method": "ms.channel.connect", "params": { "token": "<saved token>" } }
```

The reply carries `data.token` on 2018+ firmware. Present that token next time
and the TV does not prompt. Without persistence every reconnect shows a PIN
dialog, which is fatal for an unattended client.

**Key presses** go as `ms.remote.control` frames with `Click` commands. **App
launch** goes through DIAL on port 8080 (`/ws/apps/{id}`), which needs no
approval prompt at all; well-known ids are stable (Netflix `1110110`, YouTube
`2019010019`, Prime Video `3201512006786`).

**Idle sockets die.** A channel left open for hours is closed by the TV; a send
then fails. The fix is a pooled channel with one reopen-and-retry, which is what
`TizenConnectionPool` does — not a reconnect per key press, which would re-prompt
on sets without a stored token.

**What is not implemented:** token derivation for enterprise panels and AES-GCM
payload framing on newer firmware — those return `Unsupported`. The self-signed
certificate is handled by `TlsPolicy::Pinned` / `TrustOnFirstUse`; see
[`architecture.md`](architecture.md#where-the-transport-crate-sits) and the TLS
section of [`../INTEGRATION.md`](../INTEGRATION.md).

**The MAC is readable before pairing.** `GET http://<ip>:8001/api/v2/` (plain HTTP, no
auth header, no token) answers a JSON document whose `device.wifiMac` is the set's
hardware address:

```json
{ "device": { "networkType": "wireless", "wifiMac": "B8:BC:5B:B1:F2:D2", "modelName": "QN55LS03RAFXZA" } }
```

Traps: `wifiMac` is **optional** on some firmware/models — parse it as optional and refuse
when absent rather than falling back to something else. It is the Wi-Fi address even when
the set is wired (`networkType: "wired"`), so a Wake-on-LAN caller should cross-check
`networkType` before assuming it is the interface that will answer. And `device.ssid` is
sometimes *itself* a MAC-shaped value — it is not a second source. Samsung does not
document this endpoint anywhere; the provenance is community reverse-engineering,
corroborated by five independent captures (samsung-tv-ws-api's `rest.py` issues it with no
token; four public debug logs carry the field).

**Typing text** goes over the remote channel, so it needs the pairing token like any other
`ms.remote.control` command. The text is **base64** and travels in `Cmd`:

**Listing installed apps works only on older sets, and only as an event.** Emit
`{"method":"ms.channel.emit","params":{"event":"ed.installedApp.get","to":"host"}}` on the
remote-control channel and a **Tizen ≤4** set (roughly pre-2020) replies with an
*unsolicited* event frame:

```json
{"data":{"data":[{"appId":"11101200001","app_type":2,"icon":"/opt/share/webappservice/apps_icon/FirstScreen/11101200001/250x250.png","is_lock":0,"name":"Netflix"}, …]},"event":"ed.installedApp.get","from":"host"}
```

Sources: `xchwarze/samsung-tv-ws-api` (`remote.py`, `event.py`), `ollo69/ha-samsungtv-smart`
(`samsungws.py`), an openHAB binding TRACE log carrying the verbatim body above, and
`Ape/samsungctl` issue #75. The `appId` values are the same ids
`GET /api/v2/applications/{appId}` takes.

Four things this fact carries, each of which has bitten a real client:

- **Not a request/response pair.** The list arrives as an event on the open socket; awaiting
  an inline answer loses it.
- **`app_type` decides the launch action** — `2` is `DEEP_LINK`, `4` is `NATIVE_LAUNCH` (a `1`
  is also seen). Sending the wrong action type fails with a WebSocket close `31522`, so the
  type must be stored alongside the id.
- **Tizen 5+ (2020+) removed the command.** The set ignores it *silently* — no error, no
  frame — so a client that waits hangs until timeout. Sources: `xchwarze` issue #23,
  `homebridge-samsung-tizen`'s APPLICATIONS.md ("Many newer TVs no longer support this
  request, so there is no list"), a FHEM thread on a 2020 set, and `samsungctl` #75 on a 2021
  model.
- **The DIAL device description does not list apps.** It carries `serviceList` and nothing
  else — confirmed against captured bodies from a 2015 UN40HU7000 and a 2020 UA50RU7400RSMZ.
  DIAL's own `/apps` endpoint on port 8080 is **unverified** on Samsung hardware: no source,
  capture or client issues it. DIAL on these sets is launch-only.

```json
{ "method": "ms.remote.control",
  "params": { "Cmd": "<base64 of the UTF-8 bytes>", "DataOfCmd": "base64", "TypeOfRemote": "SendInputString" } }
```

There is no `Option` field here, unlike `SendRemoteKey`. Some firmware wants a
`ms.channel.emit` broadcast of `custom.remote.textReceived` first, and `SendInputEnd` closes
the session — treat both as conditional, not mandatory. A password sent this way needs
`wss` on 8002 rather than plain `ws` on 8001. With no text field focused, nothing is typed
and no error frame says so. Sources: samsung-tv-ws-api `remote.py` (the base64 encoding and
the exact frame), samsungctl issue #75 (the same frame stated independently), and
ha-samsungtv-smart's `samsungws.py`.

## §4 LG webOS

**SSAP** (Second Screen Application Protocol) — a JSON-RPC-flavoured WebSocket on
port 3000 (plain) or 3001 (TLS).

The first connection registers a manifest and triggers an on-screen "Allow?"
prompt. Acceptance returns a `client-key`; send it in later `register` payloads
and the prompt never appears again.

Commands are addressed by URI rather than method name:

```
ssap://system.launcher/launch    {"id": "com.webos.app.browser"}
ssap://audio/setVolume           {"volume": 15}
ssap://audio/getVolume
ssap://system/turnOff
ssap://com.webos.applicationManager/getForegroundAppInfo
ssap://com.webos.applicationManager/listApps
```

`listApps` answers `{"apps":[{"id":"com.webos.app.netflix","title":"Netflix"}]}`: the
catalogue of installed applications, and an entry with no `id` is skipped rather than
shown as a blank row. The shape is documented for the same Lune bus by webOS OSE
(`webosose.org/docs/reference/ls2-api/com-webos-service-applicationmanager/`, which names
the method `listLaunchPoints` and lists `launchPoints[].id` and `launchPoints[].title` as
required fields) and by LG's own Application Manager reference
(`webostv.developer.lge.com/develop/references/application-manager`); an independent client
library (`hobbyquaker/lgtv2`) maps the same reply to id → title. The crate's URI is
`listApps`, the name webOS uses on consumer televisions; `listLaunchPoints` is the name
the open-source tree documents.

**Typing text** is an IME service call, and the text is plain UTF-8 — not base64, unlike
Samsung:

```
ssap://com.webos.service.ime/insertText       {"text": "hello", "replace": true}
ssap://com.webos.service.ime/sendEnterKey     {}
ssap://com.webos.service.ime/deleteCharacters {"count": 1}
```

It answers `{"returnValue": true}` or `{"returnValue": false, "errorText": "…"}`. Needs the
paired session, and a focused text field — with none focused it fails rather than typing
somewhere. `replace` is a boolean. Sources: webOS OSE's IME reference (method, params,
return shape, ACG `ime.operation`) and lgtv2's command table (the consumer SSAP URI), with
a public Home-Assistant workaround showing it working on webOS 25. The weakest of the
verified entries here: LG's *consumer* docs do not cover `com.webos.service.ime` at all,
only the open-source tree does.

**The MAC is not verified, and the honest answer is to keep refusing.** Neither
`ssap://system/getSystemInfo` nor `com.webos.service.tv.systemproperty` `getSystemInfo`
returns one — that key set is closed (`boardType`, `firmwareVersion`, `modelName`,
`sdkVersion`, `UHD`, `_3d`), and LG staff have said twice, in 2023 and 2025, that no MAC
API is provided. The only endpoint that carries one is
`ssap://com.webos.service.connectionmanager/getinfo` (`wifiInfo.macAddress` /
`wiredInfo.macAddress`), documented in the OSE tree and implemented by lgtv2 — but absent
from LG's consumer documentation, its MAC-bearing objects gated at **API level 29** (the
method itself is level 11), and lgtv2 swallows the failure so its code cannot distinguish
working from refused. That is not verified, so it is not code. Two traps either way:
`READ_NETWORK_STATE` in a pairing manifest is not evidence that `getinfo` is permitted, and
**LGUDID is not a MAC** — it is a hash seeded from one, and must never be stored in
`DeviceDescriptor::mac`.

Traps:

- One persistent session per TV. SSAP is a long-lived socket, not a request per
  call.
- Responses are matched by `id`; a session that ignores this will read another
  command's reply.
- Port 3001 uses a self-signed certificate. Do not disable verification to make
  it work: pin or record the certificate through `TlsPolicy` and let the
  connection fail closed instead.

## §5 Roku

**ECP** (External Control Protocol) — plain HTTP on port 8060, no authentication
on the local subnet. The most cooperative of the network protocols:

| Endpoint             | Method | Returns                                                               |
| -------------------- | ------ | --------------------------------------------------------------------- |
| `/query/device-info` | GET    | XML: model, serial, firmware, power mode, MAC                         |
| `/query/apps`        | GET    | installed apps with their numeric ids                                 |
| `/query/active-app`  | GET    | XML: the app in front — `Roku` with no `id` on the home screen        |
| `/keypress/{Key}`    | POST   | emulates a remote button (`Home`, `Up`, `Select`, `Back`, `PowerOff`) |
| `/launch/{appId}`    | POST   | starts an app                                                         |

`/keypress/` also carries text. Roku's ECP documentation: "The key parameter can either
be a key name … or a printable character value specified with the prefix `Lit_`. Printable
ASCII character code values can be transmitted as-is with the `Lit_` prefix" — so
`curl -d '' "http://<tv>:8060/keypress/Lit_r"` types `r`, one character per request. **The
documented surface is printable ASCII** (0x20–0x7E); the encoding for anything else is not
documented in that source, so a client must refuse a character it cannot prove rather than
guess at a percent-encoding. Source: Roku's External Control Protocol documentation
(`developer.roku.com`), with the same `Lit_` prefix confirmed by independent ECP clients.

`/query/active-app` answers `<active-app><app id="12" type="appl" version="4.1.218">Netflix</app></active-app>`:
the `app` element is the bare text `Roku` with **no** `id` when the user is on the home
screen (nothing is in front — not an application called Roku), and a `<screensaver>`
sibling appears while a screensaver covers the app. Source: Roku's External Control
Protocol documentation (`developer.roku.com`).

Traps:

- Different models expose different subsets of `<device-info>`; every field must
  be optional in your parser.
- App ids are numeric and model-dependent; resolve a name to an id at runtime
  instead of hard-coding.
- `PowerOff` is a key press, not a power API — some sets ignore it when a
  screensaver is active.
- Roku OS 14.1 gates key presses behind a "Control by mobile apps" setting; the
  client maps that refusal to an auth error naming the setting, and the quirks
  table documents it.

## §6 Samsung MDC (commercial panels)

**MDC** (Multiple Display Control) is what Samsung commercial signage panels
expose where a consumer TV would offer nothing to automate. One wire, two
carriers: TCP 1515 by default, or an RS-232 line behind the `serial` feature
(Samsung LFDs are documented at 9600 8N1). Same frames, same checksum, same
reply parser on both carriers.

**Frame** — a fixed shape with a trailing checksum:

```
0xAA | command | display id | length | data… | checksum
```

**Addressing** — every frame carries a display id, which is how one connection
serves a whole video wall: ids `0x00`–`0xFD` address individual panels,
`0xFE` is broadcast to the wall, `0xFF` is reserved for the reply header and a
valid id in a request is rejected. Clones of an `MdcClient` share the socket;
`.display(id)` changes only the addressed id.

**Commands implemented** — power (`0x11`; the sub-code also carries reboot),
input (`0x14`, plus the aggregated status command `0x00` that reads power,
volume, mute, input and aspect back), volume and mute, brightness, panel key
lock (`0x5F`), content orientation (`0xC8`/`0x82`), signal state and panel
health (`0x0D`: lamp fault, input-source error, panel temperature, fan), and
panel on-time (`0x83`, behind the core's `supporting` feature).

**Replies and errors** — a good reply is an `ACK` carrying the requested data; a
`NAK` is returned with its command and error code and **the socket stays open**,
because the wire is still in sync. A timeout or a malformed reply drops the
socket so the next command reconnects. The `0x0D` reply reports input-source
error code `0x02` for an invalid selection — a decoder reading any byte
outside the documented range is a bug, not a maximum.

**Firmware trap** — the MDC document defines the bare frame, but some TV
firmware also expects a trailing `0x0D` after it. The transport defaults to
off (matching commercial displays) and turns on with
`MdcOptions::with_line_terminator(true)`.

**What MDC does not have:** no command captures the panel's picture, so screen
capture is never advertised; no discovery, DHCP or TLS — a wired wall is a
different trust shape from the network channels above.

## §7 HDMI-CEC (panels the network cannot reach)

CEC is the HDMI bus's own remote-control channel: power on, standby and active
source travel over the cable. The crate drives it through a `cec-client`
subprocess, which keeps `unsafe` and a C toolchain out of the workspace; a
missing binary or a missing adapter is a typed `Unavailable`, not a silent
no-op. It is optional (`client` feature) and never linked into the agent's
default build.

## §8 DLNA renderer control

**UPnP MediaRenderer** — the renderer side of DLNA, implemented as transport
in `tv-rs-dlna` (not in `tv-rs-media`, which only serves bytes):

- **Discovery**: SSDP with the MediaRenderer search target; the `LOCATION`
  device description names the renderer's `AVTransport` service URL.
- **AVTransport**: SOAP actions `SetAVTransportURI`, `Play`, `Pause`, `Stop`,
  `Seek`. The URI pushes whatever the caller hands in; metadata travels as
  DIDL-Lite.
- **GENA eventing**: `SUBSCRIBE` / renew / `UNSUBSCRIBE` against the service's
  event URL; `NOTIFY` bodies carry state. Eventing needs a reachable callback
  host on the LAN.
- **Advertisement**: the crate can also _announce_ a MediaServer the caller
  hosts (alive/byebye + `M-SEARCH` responder). The server itself, the media
  library and ContentDirectory stay application-owned.

## §9 Media serving

Two behaviours a TV player will reject you for not having.

**`206 Partial Content`** — when a viewer seeks, the player sends
`Range: bytes=N-` and expects a `206` with `Content-Range: bytes N-end/total`
and `Accept-Ranges: bytes`. Serving the whole file instead makes the seek bar
dead. Support the three real-world forms (`bytes=N-`, `bytes=A-B`, `bytes=-N`),
clamp an over-long end, and answer `416` with `Content-Range: bytes */total` when
the range starts past the end.

**HLS** — an `.m3u8` playlist of segments, so the TV's own decoder handles
chunking and bitrate. The minimum a player accepts:

```
#EXTM3U
#EXT-X-VERSION:3
#EXT-X-TARGETDURATION:6
#EXT-X-MEDIA-SEQUENCE:0
#EXT-X-PLAYLIST-TYPE:VOD
#EXTINF:6.000,
seg0.ts
#EXT-X-ENDLIST
```

`TARGETDURATION` must be the rounded-up maximum segment length; a value smaller
than the longest segment makes some players refuse the playlist.

## Not implemented here

**Google Cast** — mDNS discovery, then TLS on port 8009, protobuf payloads;
the `.proto` definitions are not published, so the payloads cannot be written
from documentation alone.

**AirPlay** — Bonjour, HTTP for media, a binary RAOP channel for audio, with
PIN/FairPlay protection.

**Android TV** — ADB over TCP (`5555`) gives total control (`input keyevent`,
`am start`) but requires developer mode and a pairing step per TV. It is a
different trust model from the protocols above. A second door, researched
2026-10-23 in the matrix's peer landscape: the **Android TV Remote protocol v2**
(the Google TV app's own — mTLS + Polo pairing + protobuf on fixed 6467/6466,
pre-installed service, no developer mode). The device's name and MAC parse out
of the **server** certificate the TV presents on the pairing port — no pairing
needed to read it. Protocol facts are crystallized in the webos/androidtv
research briefs; the spike that decides between them is plan item 3.11 in
[`06-plan.md`](06-plan.md).

All three are sender protocols or separate trust models; a partial
implementation is worse than none. The deferral list lives in
[`03-matrix.md`](03-matrix.md#deferred).

### Handoff from SignageMaster (2026-10-22) — for the agent that owns this workspace

Requested by the SignageMaster refactor spec
(`Simple-Signage/docs/specs/ARCHITECTURE-REFACTOR-SPEC.md`, cards E6 and E3b). None of
it exists here yet; each item states what moves in, and the boundary it must keep.

**TV app producer (E6)** — builds a Tizen `.wgt` / webOS app directory / plain web
bundle around a supplied payload. It is open work with its own exit criteria:
[plan item 2](06-plan.md). The protocol facts it needs — manifest fields and the
D-pad key codes — are in §3 (Tizen) and §4 (webOS) above.

**Display probe merge (E3b)** — SignageMaster's `device-hardware/src/display/*`
(~800 lines, EDID/connector enumeration) overlaps `tv-rs-local`. Compare the two;
port anything `tv-rs-local` lacks, then SignageMaster depends on `tv-rs-local`
and deletes its copy. Audio, USB and sensor probes are **not** moving here.
Gaps found 2026-10-23 (SignageMaster path is now `crates/device/src/hardware/display/`):
`tv-rs-local` has no public EDID byte parser (SignageMaster's `parse_edid_bytes` in
`edid.rs` returns product code, serial and width/height in cm; `tv-rs-local`'s
`parse_edid` is private and Linux-only) and no Windows registry EDID read. Both are
now landed — see [plan item 2b](06-plan.md) and the CHANGELOG — leaving the consumer
switch as the open half.

**Streaming boundary** — SignageMaster is extracting a separate `streaming-core`
workspace (MJPEG, HLS, WebRTC, RTSP relay). `tv-rs-media` stays the minimal byte server
described in §9; it does not depend on `streaming-core`, and `streaming-core` does not
depend on tv-rs. The overlap between the two HLS playlist writers is accepted and
revisited once both are published.
