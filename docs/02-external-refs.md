# References and prior art

Where the protocol facts came from, and which existing crates shaped the design.

## Protocol references

| Topic | Reference |
|---|---|
| SSDP / UPnP device architecture | UPnP Device Architecture 1.1 §1 (discovery, description, `M-SEARCH`) |
| Wake-on-LAN | AMD Magic Packet whitepaper — the 6×`0xFF` + 16×MAC layout |
| Samsung remote channel | community-documented SmartView/legacy remote API: `ws://:8001` and `wss://:8002` under `/api/v2/channels/samsung.remote.control`, `ms.channel.connect` handshake |
| Samsung app ids | public identifiers: Netflix `1110110`, YouTube `2019010019`, Prime Video `3201512006786` |
| DIAL | Netflix/Google DIAL spec — `POST /ws/apps/{id}` on port 8080 |
| LG webOS SSAP | webOS second-screen documentation and the `ssap://` URI namespace (`system.launcher/launch`, `audio/setVolume`, …) |
| Roku ECP | Roku External Control Protocol docs: `/query/device-info`, `/query/apps`, `/keypress`, `/launch` on 8060 |
| HTTP range requests | RFC 7233 — `Range`, `Content-Range`, `206`, `416` |
| HLS | RFC 8216 — `#EXTM3U`, `#EXT-X-TARGETDURATION`, `#EXT-X-ENDLIST` |

Ports and payloads are also summarised per protocol in
[`01-playbook.md`](01-playbook.md), which is the version this code was written
against.

## Prior art in Rust

| Crate | What it taught us |
|---|---|
| `samsung-tv` | a working shape for a Tizen client: `tokio-tungstenite` for the WSS channel, `reqwest` for REST, `serde_json` for the frames. We took the dependency shape, not the code, and replaced its TLS handling with a policy type. |
| `roxmltree` | the reason UPnP/ECP XML parsing here is a pure function over a borrowed document rather than a `serde` derive. |
| `tokio-tungstenite` | the connector design: TLS is supplied by the caller, which is what makes one policy type viable across two vendors. |
| `axum` | the media server holds no state beyond a config struct, which keeps `206` handling testable at the socket level. |

## What is deliberately borrowed from none of them

- **No shared "TV trait" across vendors.** Three protocols with different
  handshakes behind one trait produces a lowest common denominator that fits
  none of them; the facade covers the intersection and stops.
- **No automatic retries beyond one reopen.** A retry loop against a sleeping TV
  turns a clear failure into a timeout.
- **No credential storage in the library's own format.** Tizen tokens and webOS
  client keys are written by the caller-chosen path the crate is handed; the
  crate never picks a location or a permission model for a secret.
