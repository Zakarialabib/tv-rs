# tv-rs-transport

Shared networking, TLS configuration, and resilient frame reading for TV protocol clients.

## Capabilities

- **TLS Policy**: Centralized TLS connector supporting `rustls` (default) and `native-tls` (opt-in).
- **Certificate Pinning**: TOFU (Trust On First Use) and certificate fingerprint pinning for self-signed TV certificates.
- **Budgeted Frame Reader**: Protects WebSocket consumers from stalled frames or unbounded payloads.
- **Unified Unsupported Error**: Common vocabulary for unavailable protocols or capabilities across backends.
