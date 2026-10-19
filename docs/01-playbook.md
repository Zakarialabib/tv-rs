# TV Control & Communications Playbook (Full)

Source content: pasted playbook from user (Qwen 3.8 Max). Organized by section number matching original.

---

## 1. Network Discovery & Enumeration (The Foundation)

Before any control, locate TV on LAN and identify make/model/capabilities.

- **SSDP (Simple Service Discovery Protocol)** — UDP port 1900, M-SEARCH broadcast. TVs respond with LOCATION header → XML Device Descriptor (model, friendly name, API endpoints). Used by UPnP, Samsung Tizen, LG webOS.
- **mDNS / Bonjour (Port 5353)** — Apple AirPlay, Google Cast (Chromecast), some Android TV. Multicast announcements: `_googlecast._tcp.local`, `_airplay._tcp.local`.
- **ARP Scanning** — Resolve TV IP → MAC for Wake-on-LAN before deep sleep.

## 2. Power Management (Wake & Sleep)

- **Wake-on-LAN (WoL) / Wake-on-Wireless-LAN (WoWLAN)** — UDP "Magic Packet" (MAC repeated 16 times) to port 9. Only reliable hard-off wake method.
- **Power State Polling** — REST/WebSocket pings to check main CPU awake vs network-only standby.

## 3. Samsung Tizen OS (WebSocket & DIAL)

- **WebSocket API:**
  - Legacy (Pre-2018): Port 8001 (HTTP/WS, unencrypted)
  - Modern (2018+): Port 8002 (TLS/WSS) — requires strict certificate handling
- **Handshake:** `{"method":"ms.channel.connect","params":{"token":"<saved_token>"}}`. No token = PIN displayed on screen.
- **DIAL (Discovery And Launch)** — REST protocol (Netflix/Google). DIAL server usually port 8080. POST XML payload to launch native apps (YouTube, Netflix) without user prompt.

## 4. LG webOS (SSAP Protocol)

- **SSAP (Second Screen Application Protocol)** — proprietary.
- **WebSocket Endpoints:**
  - Port 3000 (Legacy HTTP/WS)
  - Port 3001 (Modern TLS/WSS)
- **Communication Model:** JSON-RPC framing. Commands to specific URIs: `ssap://system.launcher/launch`, `ssap://audio/setVolume`.
- **Pairing:** First connection triggers on-screen prompt ("Allow [Device Name]?"). Once accepted, TV returns `client-key` — must persist in Rust backend.

## 5. Roku OS (ECP — External Control Protocol)

- Most "hacker-friendly" OS — unauthenticated local-network REST API.
- **Port 8060** — HTTP server on every Roku device.
- **Endpoints:**
  - `GET /query/device-info` — XML response: firmware, MAC, screen resolution.
  - `POST /keypress/PowerOff` — simulates remote button press.
  - `POST /launch/12` — launches app by numeric ID.
- **Security:** No authentication if request from local subnet.

## 6. Android TV / Google TV (ADB over TCP/IP)

- **Port 5555** — standard ADB TCP/IP.
- **Prerequisite:** Developer Options + ADB Debugging over Network enabled on TV.
- **Capabilities (shell commands):**
  - `input keyevent KEYCODE_HOME`
  - `am start -n com.netflix.ninja/.MainActivity` (launch apps)
  - `cmd display` (resolution + HDR)

## 7. Universal Media Rendering (UPnP / DLNA)

- **SOAP / XML:** HTTP POST with XML bodies (SOAP envelopes).
- **AVTransport Service:** Core playback service. `SetAVTransportURI` (media server URL) → `Play`.
- **RenderingControl:** Volume / mute adjustments.

## 8. Media Streaming & Seeking Mechanics

- **HTTP 206 Partial Content:** TV native players demand this. User seeks → TV sends `Range: bytes=...` header. Rust server must calculate byte offset, seek file, return `206 Partial Content` with correct `Content-Range` header.
- **HLS (HTTP Live Streaming):** `.m3u8` manifest preferred over raw MP4; native decoder handles chunking.

## 9. Proprietary Casting Protocols (Google Cast & AirPlay)

- **Google Cast:** mDNS discovery → TLS connection on port 8009. Serialized Protocol Buffers (Protobuf).
- **AirPlay:** Bonjour discovery → HTTP for photo/video pushing. Proprietary binary protocol (RAOP) for audio mirroring, often protected by FairPlay DRM or PIN authentication.

---

## 10. The "Black Box": What We May Miss & Why We Need REA

Publicly documented protocols (above) are the base. Total control requires undocumented/encrypted/proprietary layers.

### Gap A: SSL Certificate Pinning & WSS Handshakes
- **Problem:** Modern Samsung (Port 8002) and LG (Port 3001) use WSS with self-signed certificates → TLS handshake fails with standard Rust clients.
- **REA Solution:** Decompile Samsung SmartThings and LG ThinQ APKs (JADX). Find exact Java/Kotlin `TrustManager` / SSL socket factory code. Extract certificate validation bypass or hardcoded self-signed CA certificate → allows `tokio-tungstenite` client to connect seamlessly.

### Gap B: The "Token Generation" Math
- **Problem:** Enterprise/high-end commercial TVs require cryptographic token from TV MAC + hardcoded secret salt.
- **REA Solution:** Feed official control app APK into REA's JADX analyzer. Prompt AI: *"Find authentication header generation function. Extract hashing algorithm, salt. Write equivalent Rust code."*

### Gap C: Undocumented Deep-Link Intents & Payloads
- **Problem:** ECP (Roku) and SSAP (LG) have documented commands (Netflix, YouTube). Hidden payloads for hidden web browser launch, HDMI input force, local file index refresh are unknown.
- **REA Solution:** Analyze `.har` / `.pcap` captures recorded during manual QA button presses in official mobile app. REA AI maps UI presses → exact JSON payloads / REST endpoints → generates `serde` structs for Rust crate automatically.

### Gap D: Firmware-Specific Quirks (The "Matrix" of TVs)
- **Problem:** 2019 Samsung accepts plain-text token; 2024 encrypts WebSocket payload with AES-GCM (key derived during pairing).
- **REA Solution:** Download older + newer official manufacturer APK versions. Compare binaries. Prompt AI: *"Analyze WebSocket connection class differences between v2.1 and v3.0. Generate Rust trait handling legacy plain-text auth and modern AES-GCM encrypted framing."*

### Gap E: Proprietary Protobuf Schemas (Google Cast)
- **Problem:** Google Cast uses compiled Protocol Buffers. `.proto` definition files not published by Google.
- **REA Solution:** Feed Chromecast-enabled Android APK into REA. AI locates compiled Protobuf classes, reverse-engineers field mappings, outputs `.proto` files → compile with `prost-build` in Rust workspace.

---

## Execution Plan Summary

1. **Phase 1:** Build Rust skeleton using known protocols (ECP, standard WebSocket, SSDP).
2. **Phase 2 (REA Phase):** Target official Android APKs (SmartThings, ThinQ, Roku). REA automates SSL bypass, token math, hidden JSON schema extraction.
3. **Phase 3:** Feed REA-generated Rust structs and cryptographic functions back into crate → 100% parity with official mobile apps.
