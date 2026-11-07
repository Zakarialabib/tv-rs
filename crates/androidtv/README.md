# tv-rs-androidtv

Android TV / Google TV remote protocol v2 (ATR v2) client and verification spike (`publish = false`).

## Overview

- Speaks the Google TV Remote Protocol v2 over mTLS with Polo pairing handshake (port 6467).
- Encodes and decodes Protobuf `RemoteMessage` control frames (port 6466).
- Probes model, name, and MAC identity parsed directly from server certificates without requiring ADB or developer mode.
- Includes automated probe runner: `cargo run -p tv-rs-androidtv --example probe -- <host> [pairing-code]`.
