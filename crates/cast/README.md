# tv-rs-cast

Google Cast SENDER implementation (control of real Chromecasts / Google TV targets).

- Receiver emulation is a non-goal.
- Discovery (mDNS _googlecast._tcp) already exists in `tv-rs-discovery` and should be reused.
- Control channel uses a TLS client to port 8009 carrying length-prefixed CASTV2
  protobuf messages; the device's self-signed certificate is accepted (no pin yet).
- Provides DIAL fallback on HTTP port 8008.
