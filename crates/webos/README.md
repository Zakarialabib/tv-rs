# tv-rs-webos

LG webOS control over SSAP, the JSON-RPC-ish WebSocket service every LG set
exposes: plain on port `3000`, TLS-wrapped on `3001`. This crate owns the
handshake, the pairing prompt, session reuse and the pointer channel; `tv-rs`
maps it onto the core capability traits.

```rust,no_run
use tv_rs_webos::{LiveSessionFactory, WebosClient, WebosSessionPool};

# async fn run() -> tv_rs_webos::Result<()> {
let client = WebosClient::new("192.168.1.20");
let pool = WebosSessionPool::with_factory(Box::new(LiveSessionFactory));

pool.send_key(&client, "HOME").await?;
let muted = pool.get_mute(&client).await?;
let apps = pool.list_apps(&client).await?;
println!("muted: {muted}, {} apps installed", apps.len());
# Ok(())
# }
```

# Pairing, and why sessions are pooled

The first connection raises an on-screen "Allow?" prompt, and the TV answers with a
`client-key` that should be persisted (`save_client_key`) and reused
(`load_client_key`) on every later connection. A connection *without* one re-raises
the prompt — which is why `WebosSessionPool` exists rather than a dial per command:
one registered session per television, reopened once when a request fails, and
dropped by `clear()` when a mobile app is backgrounded.

# Key input is a second socket

SSAP has no remote-key message. A press is a `type:button` frame written to a
*second* WebSocket whose URL `getPointerInputSocket` returns:

```
type:button
name:HOME

```

`PointerFrame` builds that frame, `PointerSocket` dials the URL verbatim (the TV
issues a path per request), and `PointerWire`/`PointerFactory` exist so a pool can
be tested without a television. `WebosSessionPool::send_key` keeps one pointer
socket per TV and reuses it across presses — re-dialing per key would make a remote
unusable.

The registration manifest asks for `CONTROL_INPUT_TEXT` and
`CONTROL_MOUSE_AND_KEYBOARD` (`WEBOS_DEFAULT_PERMISSIONS`). Without them the input
service refuses the pointer path, and the request fails even though registration
succeeded.

# What it answers

| Request                                | Method                                    |
| -------------------------------------- | ----------------------------------------- |
| Volume read / set                      | `pool.get_volume` / `set_volume`           |
| Mute read / set                        | `pool.get_mute` / `set_mute`               |
| Input switch (`HDMI_1`, `tv`, …)       | `pool.switch_input`                        |
| Installed apps                         | `pool.list_apps` → `InstalledApp`          |
| Foreground app                         | `pool.foreground_app`                      |
| Launch, open a URL, turn off           | `pool.launch_app`, `open_url`, `power_off` |

# What it deliberately does not do

- **It cannot say which input is live.** webOS exposes no reliable read, so the
  facade answers `None` rather than guessing — an unknown dot beats a wrong one.
- **It does not translate input ids.** `switch_input` takes the webOS id; mapping
  `Input::Hdmi(2)` onto `HDMI_2` is the facade's job.
- **It does not retry the pointer channel forever.** A dead pointer socket is
  reopened exactly once, the same rule the session pool uses.

# Development

- **Tests:** `cargo test -p tv-rs-webos`. The pool's reuse and reopen-once
  behaviour is proved against counting doubles, not a television.
- **No wrap:** `cargo fmt --all --check` must be clean.

Nothing here has been verified against a real panel; the wire shapes are pinned by
tests. Say so before claiming a set works.
