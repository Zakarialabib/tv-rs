# Protocol playbook

What each TV protocol actually does, at the level of detail you need to write or
debug a client. Ports, handshakes, payload shapes, and the traps that cost an
afternoon.

This page is the reference the crates were built from. It is deliberately
protocol-only: no vendor app is described here, and nothing depends on one.

## Discovery

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

## Power

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

## Samsung Tizen

**Remote channel** — a WebSocket at `/api/v2/channels/samsung.remote.control`:
- `ws://<ip>:8001` on legacy sets (plaintext),
- `wss://<ip>:8002` on 2018+ sets (TLS, self-signed certificate).

The name is passed as a query parameter, base64-encoded, and identifies the
client in the TV's on-screen prompt.

**Handshake**

```json
{"method":"ms.channel.connect","params":{"token":"<saved token>"}}
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

**What is not implemented:** certificate pinning for the self-signed TV cert,
token derivation for enterprise panels, and AES-GCM payload framing on newer
firmware. Those return `Unsupported`.

## LG webOS

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
- Port 3001 uses a self-signed certificate, so a strict TLS client fails there.
  Do not disable verification to make it work — report the gap instead.

## Roku

**ECP** (External Control Protocol) — plain HTTP on port 8060, no authentication
on the local subnet. The most cooperative of the three:

| Endpoint | Method | Returns |
|---|---|---|
| `/query/device-info` | GET | XML: model, serial, firmware, power mode, MAC |
| `/query/apps` | GET | installed apps with their numeric ids |
| `/keypress/{Key}` | POST | emulates a remote button (`Home`, `Up`, `Select`, `Back`, `PowerOff`) |
| `/launch/{appId}` | POST | starts an app |

Traps:
- Different models expose different subsets of `<device-info>`; every field must
  be optional in your parser.
- App ids are numeric and model-dependent; resolve a name to an id at runtime
  instead of hard-coding.
- `PowerOff` is a key press, not a power API — some sets ignore it when a
  screensaver is active.

## Media serving

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

## Casting (not implemented)

**Google Cast** — mDNS discovery, then TLS on port 8009, protobuf payloads.
**AirPlay** — Bonjour, HTTP for media, a binary RAOP channel for audio, with
PIN/FairPlay protection.

Both are out of scope here: they are sender protocols with their own payload
schemas, and a partial implementation is worse than none.

## Android TV (not implemented)

ADB over TCP (`5555`) gives total control (`input keyevent`, `am start`,
`cmd display`) but requires developer mode on the device and a pairing step per
TV. It is a different trust model from the three protocols above, which is why it
is not part of this workspace.
