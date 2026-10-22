# Integrating `tv-rs`

How to put this crate into an application that has to keep screens on, in the order
you will actually need it. Every section says what to call, what it costs, and what
it refuses to do.

The short version:

1. discover, and keep the descriptors — they are the only identity you get for free;
2. build a client, and pin the certificate on first use;
3. drive it through `DeviceHandle`, not through the vendor enum, if you have more
   than one screen;
4. let `tv-rs-agent` own "is everything still showing what it should be";
5. run `cargo xtask hw-test` against reality before you believe any of it.

---

## 1. Discovery: what you get, and what you do not

```rust,no_run
let tvs = tv_rs::discover().await?;
for tv in &tvs {
    println!(
        "{} | {} | {} | model {:?}",
        tv.vendor.as_str(),
        tv.ip,
        tv.label(),
        tv.model_name
    );
}
```

Discovery is SSDP: one multicast `M-SEARCH`, UDP 1900, a timeout you choose. It
needs no credentials and no pairing, which is why it is safe to call on a timer.

**What you get:** an IP, a vendor guess, and — when the device answered with a
`LOCATION` — the UPnP descriptor fields (`friendlyName`, `manufacturer`,
`modelName`, `serialNumber`).

**What you do not get:** a MAC address, a firmware version, or a stable identity
across reboots. That matters:

- `wake` needs a MAC, so nothing advertised by discovery can wake a panel. Keep your
  own MAC table keyed by IP or serial and call `tv-rs-power` yourself.
- The quirks table matches on model prefix, and the model is often enough — see §4.

**If SSDP returns nothing**, the network is the problem, not the crate: AP isolation
and VLAN separation are the usual causes, and multicast on 239.255.255.250 is what
they block. A static address list plus `TvClient::from_discovered` with a
hand-built `DiscoveredTv` is a first-class path, not a workaround.

---

## 2. One client, honest capabilities

```rust,no_run
use tv_rs::core::DeviceHandle;

let client = tv_rs::TvClient::from_discovered(tv)?;
println!("{:?}", client.capabilities());          // what this panel will accept
println!("{:?}", client.power_observation().await?); // value + confidence
```

`capabilities()` is the answer to "why is that button disabled". It is derived from
the vendor, not from a probe, and the same set gates the trait methods: asking for
something outside it returns `TvError::Unsupported` with a reason.

`power_observation()` returns the state **and how sure the crate is**:

| backend      | confidence | why                                                                   |
| ------------ | ---------- | --------------------------------------------------------------------- |
| Roku         | `High`     | ECP reports the power mode                                            |
| Tizen, webOS | `Inferred` | a TCP probe proves the control port answers, not that a picture is on |

Use `power_state()` when you only need the value — it is the same call with the
confidence dropped, so the two can never disagree.

### Commercial panels: the LFD surfaces

A 40-inch wall panel answers different questions than a consumer TV — _can nobody
walk up and change the picture_, _which way is the content turned_, _is a signal
even arriving_. On an MDC-backed handle (`tv_rs_mdc::MdcDevice`) those are real
accessors:

```rust,no_run
use tv_rs::core::{DeviceHandle, PanelHealthProvider};

# async fn example(handle: &dyn DeviceHandle) -> Result<(), tv_rs::core::TvError> {
let lock = handle.as_panel_lock().expect("panel lock");   // or handle None first
lock.set_panel_lock(true, tv_rs::core::CommandId::new()).await?;
let health = handle.as_panel_health().expect("panel health");
let observed = health.panel_health().await?;
#     Ok(())
# }
```

The rule these obey — and the one to copy in your own code — is: **an absent
accessor means the protocol cannot do it**, knowable before any command is sent
and the same fact as `Unsupported` after one. Which backend advertises what is
the one table in this repo:
[`docs/03-matrix.md` §Capability truth](docs/03-matrix.md#capability-truth) —
the consumer channels return `None`, MDC returns `Some`, and panel _hours_ and
screen _capture_ ride the `supporting` feature there (screen capture exists
nowhere on the wire and is advertised by nobody).

---

## 3. TLS: pin on first use, then never again

Retail TVs present self-signed certificates. The two options that keep validation on:

```rust,no_run
use tv_rs::transport::{cert_sink, TlsPolicy};
use tv_rs::{DiscoveredTv, TvClient};

async fn pair(tv: &DiscoveredTv) -> Result<(), Box<dyn std::error::Error>> {
    // First connection: accept what the panel presents, and record it.
    let sink = cert_sink();
    let client = TvClient::from_discovered_with(tv, TlsPolicy::TrustOnFirstUse(sink.clone()))?;
    let _ = client.power_state().await?;              // this is the handshake

    let fingerprint = {
        let guard = sink.lock().expect("sink is not poisoned");
        guard.as_ref().copied().ok_or("the handshake recorded no certificate")?
    };
    std::fs::write("living-room.pin", fingerprint.to_hex())?;

    // Every later connection: exactly that certificate, nothing else.
    let stored = std::fs::read_to_string("living-room.pin")?;
    let pinned = TlsPolicy::pinned(stored.trim()).map_err(|error| format!("bad pin: {error}"))?;
    let _client = TvClient::from_discovered_with(tv, pinned)?;
    Ok(())
}
```

Store the pin **per device**, not per installation, and store the fingerprint — not
the certificate. A firmware update may regenerate the certificate; the connection
then fails closed, and the fix is to re-run the first-use step, which is exactly the
behaviour you want on a screen someone could unplug and replace.

`TlsPolicy::AcceptInvalid` exists for debugging only. Every path to it is in one
function in `tv-rs-transport`, which is the point.

---

## 4. Ports and quirks: stop paying for dead ports

Tizen moved from `ws://:8001` to `wss://:8002`, webOS from `ws://:3000` to
`wss://:3001`. Both crates negotiate: try the modern port, fall back only when the
connection was _refused_ (a certificate mismatch is an answer, not a reason to
downgrade to plaintext), and remember what worked.

When the generation is already known, the quirks table removes the dead port from the
ladder entirely:

```toml
[dependencies]
tv-rs = { version = "0.2", features = ["quirks"] }
```

With the feature on, `TvClient::from_discovered` consults `quirks/default.toml` and
pins the port a matching row names. A `QN90*` panel is built on 8002 only; a webOS 23
panel on 3001 only. Without the feature the ladder still works — it just spends one
refused connection learning what the table already knew.

The table is data, not code: a new firmware defect is a TOML row and a review. See
[`QUIRKS.md`](QUIRKS.md).

---

## 5. A fleet, not a television

If you have more than one screen, stop calling `TvClient` in a loop. Put the clients
behind `Arc<dyn DeviceHandle>` and let the agent decide:

```rust,no_run
use tv_rs_agent::{Reconciler, StaticFileProvider};

let provider = std::sync::Arc::new(StaticFileProvider::new("desired.json"));
let reconciler = Reconciler::new(handles, provider);

// The loop runs until this sender is dropped or fired, and returns what it saw.
let (stop, stopped) = tokio::sync::oneshot::channel();
let outcomes = reconciler.run(stopped).await;
let _ = (stop, outcomes);
```

`desired.json` is a map of device id to desired state:

```json
{
  "tv-01": { "power": "on", "app": "netflix" },
  "tv-02": { "power": "on", "app": "youtube" }
}
```

Three properties matter when you build this yourself:

- **The decision is a pure function.** `decide(desired, observed, capabilities)`
  returns `Nothing | Launch | Sleep | Wake | ProbePower | Quarantine` and touches no
  I/O. You can unit-test your policy without a network.
- **Unverified writes are never `High`.** A launch that returned HTTP 200 is
  `Inferred`; only a read is `High`. That is what stops a fleet reporting success it
  never checked.
- **Quarantine is terminal for the pass, not for the day.** A device that cannot do
  the job leaves rotation, and the log says why. `HealerLadder::new(3)` is the retry
  budget; a missing credential never consumes it (`Reprovision` instead).

Providers: `StaticFileProvider` (a file), `HttpPollProvider` (feature `http`, a JSON
manifest over HTTPS), `MqttSubscribeProvider` (feature `mqtt`, `<prefix>/+/desired`,
plain TCP — see the type's docs for why).

The audit log is JSONL, one line per attempt: `ts`, `device`, `decision`, `result`.
It is readable with `tail` and loadable with anything that reads JSON.

---

## 6. Media: what a TV player insists on

TV players seek. That means `206 Partial Content` with correct `Content-Range`, and
`416` when the range is unsatisfiable — `tv-rs-media` implements both plus HLS
playlists:

```rust,no_run
use tv_rs_media::{serve, MediaServerConfig};

let config = MediaServerConfig::new("/srv/media");
let addr = std::net::SocketAddr::from(([0, 0, 0, 0], 8080));
serve(config, addr).await?;
```

`tv-rs-media` is the only crate that pulls `axum`. It is deliberately independent of
the protocol crates: a media server and a remote control have no reason to share a
dependency graph.

---

## 7. Panels the network cannot reach

If the screen has no IP — a consumer set on the wall, an HDMI matrix, a panel whose
control features the vendor removed — `tv-rs-cec` sends HDMI-CEC frames through
`cec-client`:

```rust,no_run
use tv_rs_cec::{CecAddress, CecClient, ClientTransport};

let cec = CecClient::new(ClientTransport::new("cec-client"));
cec.power_on(CecAddress::Tv).await?;
```

Needs `cec-client` on `PATH` and a CEC-capable adapter. Missing either is a typed
`Unavailable`, never a silent no-op — a technician should be told the cable is
missing, not shown a green light.

---

## 8. Before you ship

```bash
cargo xtask hw-test --timeout 5 --write-doc

cargo run -p tv-rs-cli -- status

cargo run -p tv-rs-cli -- ensure --config desired.json --audit audit.jsonl
```

`status` prints power **and its confidence**, plus the capability set for each
device. If a column looks wrong, the confidence says whether to trust it: `inferred`
means "the port answered", not "the screen is on".

`docs/hil-results.md` is the record of what has been tried against real hardware.
Today it is empty. Treat every protocol claim in this repository as documented-and-
tested-against-mocks until that table has rows.

---

## 9. Where the seams are

| You need                                                | Reach for                                                                            | Not                                            |
| ------------------------------------------------------- | ------------------------------------------------------------------------------------ | ---------------------------------------------- |
| one operation on one screen                             | `TvClient`                                                                           | the vendor crates directly                     |
| the same operation on many                              | `DeviceHandle` traits                                                                | matching on `TvVendor`                         |
| vendor-specific extras                                  | `as_tizen()` / `as_webos()` / `as_roku()`                                            | feature-detecting at runtime                   |
| panel lock / orientation / health on a commercial panel | `as_panel_lock()` / `as_orientation()` / `as_panel_health()` on an MDC-backed handle | faking a read the consumer channel cannot make |
| fleet policy                                            | `tv-rs-agent`                                                                        | a retry loop per device                        |
| video                                                   | `tv-rs-media`                                                                        | a hand-rolled range handler                    |
| a panel with no IP                                      | `tv-rs-cec`                                                                          | pretending the network one is down             |

The facade holds no protocol logic; the vendor crates hold no policy; the agent holds
no transport. Behaviour that does not fit one of those rows usually means the
integration is reaching through a seam that exists for a reason.
