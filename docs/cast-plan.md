# Implementation Plan: Google Cast sender (`tv-rs-cast`)

Status after the first slice (this PR). What shipped, what did not, and what
comes next — each item states its proof, so nothing here has to be re-learned.

## Shipped (this PR)

1. ✅ CASTV2 protobuf binding: `crates/cast/proto/cast_channel.proto` compiled
   at build time with `prost-build` (`src/proto.rs`), round-trip tested
   (`tests/test_proto.rs`).
2. ✅ Transport: TLS + length-prefixed `CastMessage` frames on port 8009
   (`src/transport.rs`). **The EKE2 claim in the original plan was not
   implemented** — the wire works as plain TLS with the device's throwaway
   self-signed certificate accepted (a `_NoVerify` verifier). The plan's
   x25519/AES-CBC framing exists nowhere in `src/`. Every doc claiming it
   (crate lib docs, README) was corrected in this slice; the verification
   gap below owns the residual risk.
3. ✅ `CastClient` (`src/client.rs`): connect, status, launch, load_media,
   transport (play/pause/stop), set_volume, stop.
4. ✅ DIAL HTTP fallback on port 8008 (`src/dial.rs`).
5. ✅ Payload JSON schemas for the connection/heartbeat/receiver/media
   namespaces (`src/payload.rs`), round-trip tested (`tests/test_payload.rs`).
6. ✅ Workspace membership and the facade surface: `TvVendor::GoogleCast`,
   `DeviceOs::GoogleCast`, facade port 8009, and a gate-dated `cast` feature
   that pulls `tv-rs-cast` in.

## Honest gaps (found while reviewing this PR)

- **Eight dead dependencies** were declared in `crates/cast/Cargo.toml` and used
  nowhere (`x25519-dalek`, `aes`, `cbc`, `hmac`, `sha2`, `pbkdf2`, `rand`,
  `async-trait`) — leftovers of the EKE2 plan that was never implemented.
  **Resolved in this slice**: all eight removed; `Cargo.lock` dropped their
  entire subtree and `clippy --all-targets -D warnings` stays clean.
- **The client cannot be tested past the frame layer.** `CastTransport` is a
  concrete TLS stream, so `status()`/`launch()`/etc. have no test cover. Make
  the transport a trait (the pattern the workspace already proves with
  `tv-rs-tizen --features testkit`) and add a fake that replays recorded
  receivers.
- **`status()` can spin forever.** The read loop that waits for
  `RECEIVER_STATUS` has no timeout or bound; a device that never answers hangs
  the call. Bound it (max-N or deadline) when the trait lands.
- **No heartbeat keepalive.** `HeartbeatRequest` (PING/PONG) ships as schema and
  is never sent; a real session would be dropped in ~5–10 s of silence. Wire
  the PING task into `CastClient`.
- **Verification sore thumb.** `NoVerify` accepts any certificate — Cast is
  like that today, but the workspace has TLS hygiene (`TlsPolicy`,
  pinning, platform-verifier in `tv-rs-transport`). Either document why Cast is
  an exception here, or reuse the policy so the crate agrees with the rest.
- **CI does not compile the crate alone.** The `features` job checks every
  published crate in isolation; add `cargo check -p tv-rs-cast` (and
  `--no-default-features` if it grows one) so a hidden sibling-feature
  dependency cannot hide behind workspace unification.
- **The facade's `GoogleCast` variant is unreachable.** `identify_vendor()` is
  SSDP-heuristic and never produces it, and `TvClient::connect` _deliberately_
  refuses it — so today the enum can only be built by a caller's hands. Either
  mDNS discovery classifies Cast devices into `TvVendor::GoogleCast` (the scan
  already exists in `tv-rs-discovery/src/cast.rs`), or the variant waits until
  the facade wiring below.
- **Cargo members redundancy.** Root members lists `crates/cast` explicitly
  while `crates/*` already matches it — the explicit entry is harmless but
  misleading next to the "no glob" story in `01-architecture.md`.
- **Doc-before-code is out.** The README claimed an EKE2 handshake that never
  existed; **corrected in this slice** (README + crate docs now describe the
  TLS + CASTV2 wire as it is). Any later claim in this plan must exist in
  code before it is written here.

## Next (replan)

1. ~~**Prune truth**: remove the eight dead deps and correct the exact
   `EKE2` sentences anywhere that still claims them.~~ — **done in this
   slice** (deps removed, README/lib docs corrected, checks re-run green).
2. **Testable client**: transport trait → `testkit` mock transport → replay
   tests; add a timeout/bound to `status()`. **Do this first — it unblocks
   everything below.**
3. **Keepalive**: PING loop in `CastClient`; document the interval behaviour.
4. **Facade wiring option A (thin)**: under the facade's `cast` feature a
   `TvVendor::GoogleCast` entry to `CastClient::connect` handoff, with a
   `DeviceOs::GoogleCast` capability table (app + volume only — the keymap has
   no Cast equivalent). Option B (delay): leave the refusal in place and
   expand the CLI's exit surface when the fleet path needs Cast.
5. **CI**: add the per-crate `cargo check -p tv-rs-cast` line to the features
   job.
6. Complete Pre Commit Steps.
7. Push and let CI arbitrate (CI also lacks a per-crate check for this crate;
   see the CI gap above).
