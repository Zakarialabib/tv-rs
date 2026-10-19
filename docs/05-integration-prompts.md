# Integration prompts

Copy-paste prompts for wiring `tv-rs` into a host application. Each block is
self-contained: it names the crates, the exact APIs, the constraints that are
easy to get wrong, and what "done" means. Paste the prompt into an agent (or a
colleague) working **inside the host repository** — this repo is the dependency,
not the place the feature lives.

Before using any prompt, add the dependency:

```toml
# LAN discovery + control only
tv-rs = { version = "0.1", default-features = false }

# with the media server (pulls axum)
tv-rs = "0.1"
tv-rs-media = "0.1"
```

Three rules every scenario inherits:

1. **Discovery is unauthenticated, control is not.** SSDP needs no credentials;
   Tizen and webOS prompt on the TV once and return a token/`client-key` that
   must be persisted and reused. Roku ECP needs nothing on the local subnet.
2. **Never call `unwrap()` on a `TvError`.** Match it: `UnsupportedVendor`
   (unknown device), `UnsupportedOperation` (vendor has no equivalent), and the
   per-crate error variants preserve the specific cause.
3. **No player is created by this crate.** It returns URLs and issues commands;
   rendering, retries and UI are the host's job.

---

## 1. Discover TVs and build an inventory

**Prompt**

> Add LAN television discovery to this project using `tv-rs`.
>
> - Use `tv_rs::discover()` for the zero-config path; use
>   `tv_rs::discover_with(&scanner)` with
>   `tv_rs::discovery::SsdpScanner::new().with_timeout(Duration::from_secs(3))`
>   when the scan needs to be bounded or bound to a specific interface
>   (`.with_interface(ipv4)` / `.with_bind_addr(addr)`).
> - Persist `DiscoveredTv { vendor, ip, friendly_name, model_name,
>   serial_number, label() }`. Identify the vendor with
>   `tv_rs::identify_vendor(&response, descriptor)` semantics — never guess from
>   the IP.
> - `TvVendor::Unknown` is a normal outcome: a device that answered SSDP but
>   matched no signature. Show it, don't drop it.
> - Refresh on demand, not on a timer; SSDP multicast is cheap but a 1-second
>   loop floods the LAN.
>
> Done when: a cold scan lists every TV on the subnet with vendor + label, an
> empty subnet returns an empty list without erroring, and a saved device
> survives without re-discovery (persist vendor + IP).
>
> Out of scope: control, wake, media.

## 2. Wake a TV before playback (and prove it woke)

**Prompt**

> Implement "wake before play" with `tv_rs::power`.
>
> - Resolve the MAC once (`MacAddress` — it accepts `AA:BB:CC:DD:EE:FF`,
>   `aa-bb-cc-dd-ee-ff` and bare hex) and persist it; discovery returns IPs, not
>   MACs, so ask for it once at setup.
> - Send the packet with `tv_rs::power::WolSender::new()?.wake(&mac)` (or
>   `wake_on_lan(&mac)` for the default broadcast). Use
>   `WolSender::with_broadcast_addr` for a directed broadcast such as
>   `192.168.1.255:9` when `255.255.255.255` is filtered.
> - Verify with `poll_power_state("ip:port")` → `PowerState::{On, Standby, Off,
>   Unknown}`. `Standby` means the NIC answers but services are down: retry,
>   don't declare failure.
> - Exponential backoff up to ~30 s, then surface a real error. WoL is
>   best-effort by design; a TV with WoWLAN disabled will never wake.
>
> Done when: the host can go from "TV off" to "control channel reachable" with
> one call, and reports a distinguishable error when the TV has no WoL support.
>
> Out of scope: retrying forever, HDMI-CEC.

## 3. Samsung Tizen — launch an app, send remote keys

**Prompt**

> Add Samsung Tizen control with `tv_rs::tizen`.
>
> - Connect with `TizenSession::connect(&TizenClient::new(host))`. On the modern
>   channel use `.with_wss()` (port 8002); the legacy plaintext channel is
>   `.with_port(8001)`.
> - **Persist the pairing token.** `client.with_token(saved)` on every later
>   connect; `session.token()` after the first handshake. Without persistence
>   the TV shows a PIN prompt on every reconnect, which is unacceptable in an
>   unattended player. `tv_rs_tizen::SamsungTokenStore` already does the file
>   part (quarantines corrupt files, never clobbers a stored token with an empty
>   one); `save_token()`/`load_token()` are the one-liners for a host that keeps
>   its own store. Find TVs with `discover_samsung(timeout)` and launch with
>   `resolve_app_id("netflix")` instead of hard-coding ids.
> - Launch DIAL apps without user interaction:
>   `client.launch_app_via_dial(app_id, payload)`; discovery of the launcher is
>   `client.dial_device_description()`.
> - Remote keys: `session.send_key(TizenRemoteKey::…)`; raw frames via
>   `send_raw`, replies via `recv_json`.
> - Map `TizenError::Unsupported` to "this TV cannot do that", not to a retry —
>   it marks the reverse-engineering gaps (certificate pinning, enterprise token
>   math, AES-GCM framing) that are deliberately unimplemented.
>
> Done when: first run prompts on the TV once, the token is stored securely,
> subsequent runs connect silently, and an app launch works headlessly.
>
> Out of scope: implementing the Phase-3 auth gaps; vendoring a bypass.

## 4. LG webOS — pairing, volume, mute, launch

**Prompt**

> Add LG webOS control with `tv_rs::webos`.
>
> - Pair once: `WebosClient::new(host).with_device_name("my-app").connect()` →
>   `WebosSession`; store `session.client_key()` with
>   `save_client_key(path, key).await`. Reconnect with
>   `.with_client_key(saved)` so the TV does not prompt again.
> - Use the typed helpers (`set_volume`, `get_volume`, `set_mute`,
>   `launch_app`, `power_off`, `foreground_app`) rather than hand-built
>   `ssap://` strings; `request(SsapUri, payload)` is the escape hatch.
> - Use `.with_wss()` for the TLS channel (3001). Certificate pinning on 3001 is
>   a known gap — if the handshake fails, surface `WebosError::Unsupported`,
>   don't disable verification.
> - Keep one session per TV; SSAP is a persistent WebSocket, not request/response
>   per call.
>
> Done when: volume/mute/launch work after a single pairing, and a second
> process start does not re-prompt the TV.
>
> Out of scope: reverse-engineering the client-key derivation.

## 5. Roku — launch by app name, send keys

**Prompt**

> Add Roku control with `tv_rs::roku`. There are no credentials: ECP is
> unauthenticated on the local subnet.
>
> - `RokuClient::new(host)`; `query_device_info()` for model/firmware/power
>   state; `query_apps()` for installed apps; `query_app_id("Netflix")` to
>   resolve a name before `launch_app(&id)`.
> - `send_keypress(RokuKey::Home | Up | Select | Back | PowerOff)` for remote
>   navigation. Keys are typed; do not build `/keypress/` paths by hand.
> - Parse nothing yourself: the XML parsers are pure functions and already
>   handle model-specific missing elements.
>
> Done when: the host can launch an app by human name and navigate its UI
> without a hard-coded channel id.
>
> Out of scope: ECP discovery beyond SSDP (the `roku:ecp` search target is
> already covered by `tv-rs-discovery`).

## 6. Serve media to a TV (206 + HLS)

**Prompt**

> Serve video to the TV from this process using `tv-rs-media`.
>
> - `MediaServerConfig::new("/srv/media")` + `serve(config, "0.0.0.0:8088".parse()?)`,
>   or mount `router(config)` into an existing axum app.
> - The TV **requires** `206 Partial Content` for seeking: the crate answers
>   `Range: bytes=N-`, `bytes=A-B` and `bytes=-N` and returns `416` with
>   `Content-Range: bytes */LEN` when unsatisfiable. Do not add a second file
>   handler in front of it.
> - For HLS, put `.ts` segments in a directory and point the TV at
>   `/hls/<dir>`; the manifest lists them sorted and advertises
>   `#EXT-X-TARGETDURATION` from `MediaServerConfig::segment_secs`.
> - Keep the server separate from the control crates: `axum` must not leak into
>   anything that talks to a TV.
>
> Done when: a player can scrub (206 responses are visible in the log) and an
> HLS manifest plays from a cold start.
>
> Out of scope: transcoding, DRM, on-the-fly segmentation.

## 7. One control endpoint for every vendor

**Prompt**

> Expose a small HTTP control API over `tv-rs` (axum or the host's framework).
>
> - Resolve a device id to a `TvClient` (persist `vendor` + `ip`, then
>   `DiscoveredTv::to_client()` or `TvClient::from_discovered`).
> - Route only what is uniform: `power_off()`, `launch_app(id)`. Everything
>   vendor-specific goes through the typed accessors —
>   `as_tizen()`, `as_webos()`, `as_roku()` — and returns `501` when the
>   accessor is `None`.
> - Map errors to status codes deliberately: `UnsupportedVendor` → 404,
>   `UnsupportedOperation` → 501, transport errors → 502. Never leak a raw
>   `Debug` string to the API surface.
> - Keep credentials (Tizen token, webOS client-key) server-side; the API takes
>   a device id, never a token.
>
> Done when: one endpoint powers off a Samsung, an LG and a Roku without
> branching in the handler, and a vendor without the capability returns a clear
> 501 instead of a panic.

## 8. Scheduled/concierge automation

**Prompt**

> Build a small scheduler that drives TVs on a timetable using the `tv-rs` CLI
> as the reference behaviour.
>
> - Reuse the CLI's command shapes (`discover`, `wake`, `power-off`, `key`,
>   `launch`) instead of inventing a second vocabulary; the CLI is the facade's
>   reference consumer.
> - Each job: resolve device → wake if unreachable → wait for the control port →
>   issue the command. Use `poll_power_state` as the readiness probe rather than
>   sleeping a fixed time.
> - Make every job idempotent (launching an app twice must be harmless) and log
>   the vendor + IP + duration, not credentials.
>
> Done when: a missed job can be re-run safely and the log answers "why did this
> TV not power off" without correlating three systems.

## 9. Test against hardware without breaking CI

**Prompt**

> Add hardware coverage that cannot run in CI.
>
> - Mirror the harness in `tests/hardware_in_the_loop.rs`: a `hil` cargo
>   feature, machine-readable env vars (`TV_RS_HIL_ROKU_HOST`,
>   `TV_RS_HIL_TIZEN_HOST`, `TV_RS_HIL_WEBOS_HOST`, `TV_RS_HIL_MAC`), and a
>   skip-with-message path when a variable is unset.
> - Default `cargo test` must compile those tests to **zero** tests; only
>   `--features hil` may run them.
> - Provide a `TV_RS_HIL_STRICT=1` mode that turns a missing device into a
>   failure, so a dedicated lab runner can gate on it.
>
> Done when: CI stays hermetic, and a developer with one TV can verify the real
> path in one command.
