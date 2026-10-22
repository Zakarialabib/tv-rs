# Deferred authentication work (internal)

Not linked from the README or the docs index on purpose. This is the note that
explains why three vendor capabilities return `Unsupported`, and what evidence
would be needed to implement them.

## What is deferred

| Gap                       | Vendor                    | What is missing                                                                                                                                                                                                                                                                                                                                                                                                                | Why it blocks                                                                                                                             |
| ------------------------- | ------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------- |
| TLS certificate pinning   | Samsung (8002), LG (3001) | **Shipping**: `TlsPolicy::TrustOnFirstUse` records the certificate a TV presents on first connect; `TlsPolicy::Pinned(fingerprint)` accepts exactly that certificate afterwards. Both need the `rustls` backend — a `native-tls` build fails those policies with `TlsError::PinningUnavailable`. The remaining gap is verification against real television certificates on hardware (plan item 5, [`06-plan.md`](../06-plan.md)). | Needs a real TV session: does the vendor client's landmark match TrustOnFirstUse, and does a pinned handshake survive a firmware renewal? |
| Token derivation          | Samsung enterprise panels | Consumer sets return a token in `ms.channel.connect`; some commercial panels expect a token derived from the device identity plus a fixed secret.                                                                                                                                                                                                                                                                              | Needs the derivation to be observed, not guessed. A wrong hash produces an auth failure that looks like a pairing denial.                 |
| Encrypted payload framing | Samsung, 2024+ firmware   | Some sets wrap WebSocket payloads in AES-GCM with a key agreed during pairing.                                                                                                                                                                                                                                                                                                                                                 | Needs the key-agreement sequence and the nonce layout.                                                                                    |
| Undocumented payloads     | Roku ECP, LG SSAP         | Documented commands cover the common verbs; input switching and hidden apps are not in the public surface.                                                                                                                                                                                                                                                                                                                     | Needs captured traffic from the vendor app to correlate UI actions with payloads.                                                         |
| Protobuf schemas          | Google Cast               | `.proto` definitions are not published, so a pure-Rust sender cannot be written.                                                                                                                                                                                                                                                                                                                                               | Needs the field mappings recovered from a sender implementation.                                                                          |

## How the code behaves today

Every one of these returns a typed `Unsupported` carrying a phase and a reason
(`tv_rs_transport::Unsupported`). The rule is that library code never panics and
never silently does the wrong thing — a caller can match the error and decide.

`TlsPolicy::AcceptInvalid` survives as an explicit, named escape hatch for
experimentation — pinning itself has landed (`TrustOnFirstUse` record on first
connect, `Pinned` from then on), so the hatch is no longer the only way to talk
to a self-signed television. Keeping it is bounded: one policy variant, one
config branch, and the diagnostics rule that a pinned or TOFU session is
preferred whenever one exists.

## If this work is ever picked up

1. **Capture first, implement second.** Traffic from the vendor's own client for
   the exact behaviour is the only reliable source; documentation and forum posts
   disagree and are usually stale by firmware revision.
2. **Keep the shape.** Whatever lands must keep returning the same typed errors
   on failure, and must not change any public signature — the crates are
   published, so a broken promise here is a semver event.
3. **Prove it against a device, not a mock.** A unit test can only assert the
   payload you already believe; the failure mode being fixed is a wrong belief.
4. **Record the firmware range.** These behaviours change between model years.
   A pinning bypass that works on a 2019 set frequently fails on a 2024 one, and
   the code needs to say which it was verified against.

## What was tried and rejected

- **Disabling verification in the connector** — works, ships a security hole, and
  hides the failure mode from the caller. Replaced by the explicit
  `AcceptInvalid` policy.
- **Vendoring vendor app code** — no. The crates speak protocols, they do not
  contain another product's implementation.
- **Shipping the native SDK** — it is proprietary and would make the crate
  non-redistributable. The FFI-free path is the whole point.
