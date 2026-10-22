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
Also not implemented; ask the user for the MAC once and store it.

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
```

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
| `/keypress/{Key}`    | POST   | emulates a remote button (`Home`, `Up`, `Select`, `Back`, `PowerOff`) |
| `/launch/{appId}`    | POST   | starts an app                                                         |

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
different trust model from the protocols above.

All three are sender protocols or separate trust models; a partial
implementation is worse than none. The deferral list lives in
[`03-matrix.md`](03-matrix.md#deferred).

### Handoff from SignageMaster (2026-10-22) — for the agent that owns this workspace

Requested by the SignageMaster refactor spec
(`Simple-Signage/docs/specs/ARCHITECTURE-REFACTOR-SPEC.md`, cards E6 and E3b). None of
it exists here yet; each item states what moves in, and the boundary it must keep.

**TV app producer (E6)** — package a supplied wasm/web payload as an installable TV
app: Tizen `.wgt`, webOS `.ipk`, and a plain web bundle. The source is
`Simple-Signage/src-tauri/crates/platform-tv-wrapper/` (197-line `lib.rs`: wasm-bindgen
glue with `boot()`/`tick(now_ms)`/`snapshot()`/`dispatch(json)`/`key_event(k, v)`, plus
`build-tizen.sh`, `build-webos.sh`, `build-tv.sh`). It depends on
`signage-master-host` and `signage-master-core-wasm` today; here it must take the
payload **generically** — no SignageMaster types, no player logic, no schedule
(see [NON_GOALS](../NON_GOALS.md): "not a CMS"). SignageMaster's no_std reducer becomes
one payload among others (agent status UI, kiosk page, test pattern). D-pad key
mapping and app-manifest generation (Tizen `config.xml`, webOS `appinfo.json`) belong
here because they are TV-protocol knowledge. Artifacts are unsigned; signing and
device install stay out of scope until a HIL row in
[`hil-results.md`](hil-results.md) exists for them. Name proposed: `tv-rs-app`. Once it
publishes, SignageMaster deletes its copy.

**Display probe merge (E3b)** — SignageMaster's `device-hardware/src/display/*`
(~800 lines, EDID/connector enumeration) overlaps `tv-rs-local`. Compare the two;
port anything `tv-rs-local` lacks, then SignageMaster depends on `tv-rs-local`
and deletes its copy. Audio, USB and sensor probes are **not** moving here.

**Streaming boundary** — SignageMaster is extracting a separate `streaming-core`
workspace (MJPEG, HLS, WebRTC, RTSP relay). `tv-rs-media` stays the minimal byte server
described in §9; it does not depend on `streaming-core`, and `streaming-core` does not
depend on tv-rs. The overlap between the two HLS playlist writers is accepted and
revisited once both are published.
