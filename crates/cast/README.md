# tv-rs-cast

Google Cast sender implementation for smart TVs and Chromecasts.

## Features

- **TLS + EKE2 Framing**: Authenticated channel on TCP 8009 using length-prefixed protobufs, X25519 key exchange, and AES-128-CBC encryption.
- **Protobuf & JSON Schemas**: Handles CASTV2 namespaces (`urn:x-cast:com.google.cast.receiver`, `urn:x-cast:com.google.cast.media`, `urn:x-cast:com.google.cast.tp.connection`).
- **Media Control**:
  - `media_status()` querying active media session state, duration, current position, and tracks.
  - Playback controls: `play()`, `pause()`, and `seek()`.
  - Volume control and receiver application launch/stop.
- **DIAL HTTP Fallback**: Dispatches launches via HTTP 8008 when the native Cast channel is unlinked.
- **Facade Integration**: Implements core `DeviceHandle` capabilities and routes directly through `TvClient` in `tv-rs`.
