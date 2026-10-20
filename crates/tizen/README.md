# tv-rs-tizen

Samsung Tizen control: the remote WebSocket channel on 8001/8002, the `DIAL` launcher
on 8080, pairing-token persistence, and the per-TV connection pool.

## Status: experimental

This crate speaks a protocol Samsung documents only as a remote-control app ecosystem.
The parts below are implemented and covered by tests **against mocks**:

- the `ms.channel.connect` handshake with token capture and reuse;
- the port ladder (modern `wss://:8002` first, legacy `ws://:8001` only when the modern
  port *refuses* — a certificate mismatch is an answer, not a reason to downgrade);
- remote-key frames, DIAL launch and app listing;
- per-read frame budgets, so a panel that accepts the socket and then goes quiet
  produces a timeout instead of a hang.

What makes it experimental is not the shape of the code but the ground under it: **no
captured transcript from physical hardware is committed here**. Test counts prove the
parsers handle the frames we believe Samsung sends; they cannot prove Samsung sends
them. `docs/hil-results.md` in the repository is the table that closes that gap, and it
is empty.

Until it has rows:

- treat a failed connection as "unknown", not "unsupported" — check
  [`TlsPolicy`](https://docs.rs/tv-rs-transport/latest/tv_rs_transport/enum.TlsPolicy.html)
  and the port the client negotiated before blaming the panel;
- run `cargo xtask hw-test` on the network where the screens live;
- if a model behaves differently, the fix is usually a row in `quirks/default.toml`
  rather than a code change — see `QUIRKS.md`.

## What it refuses

- Pre-2016 panels on `:55000`: not implemented, reported as
  `TizenError::Unsupported` rather than attempted.
- Absolute volume: Tizen accepts relative key presses only, so the crate does not
  advertise `VolumeControl` for it.
- Anything requiring the encrypted token-v3 handshake.

## Example

```rust,no_run
use tv_rs_tizen::{TizenClient, TizenRemoteKey, TizenSession};

async fn press_home() -> Result<(), tv_rs_tizen::TizenError> {
    let client = TizenClient::new("192.168.1.40").with_wss();
    let mut session = TizenSession::connect(&client).await?;
    session.send_key(TizenRemoteKey::Home).await
}
```
