# tv-rs-cast

Google Cast SENDER implementation (control of real Chromecasts / Google TV targets).

- Receiver emulation is a non-goal.
- Discovery (mDNS _googlecast._tcp) already exists in `tv-rs-discovery` and should be reused.
- Control channel uses a TLS client to port 8009 with EKE2 handshake (x25519 + AES-128-CBC over protobuf).
- Provides DIAL fallback on HTTP port 8008.
