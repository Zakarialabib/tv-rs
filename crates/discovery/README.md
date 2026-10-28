# tv-rs-discovery

SSDP `M-SEARCH` and UPnP device-descriptor parsing: the part that finds
televisions on the LAN before any protocol is spoken. It knows nothing about
vendors — that identification lives in `tv-rs`.

```rust,no_run
use std::time::Duration;
use tv_rs_discovery::SsdpScanner;

# async fn run() -> tv_rs_discovery::Result<()> {
let scanner = SsdpScanner::new().with_timeout(Duration::from_secs(2));

// Results as they arrive, so a "Find displays" screen fills in live.
scanner
    .discover_tvs_with(|response, descriptor| {
        println!("{} — {:?}", response.source, descriptor.friendly_name);
    })
    .await?;
# Ok(())
# }
```

# Two levels

| Call                                        | Answers                                  |
| ------------------------------------------- | ---------------------------------------- |
| `msearch(target, mx)`                       | raw `M-SEARCH` replies, after the window |
| `discover()`                                | `msearch` for `ssdp:all`                 |
| `fetch_descriptor(location)`                | one UPnP device descriptor               |
| `discover_tvs()`                            | replies joined with their descriptors    |
| `discover_tvs_with(on_found)`               | the same, calling back per device        |

`discover_tvs_with` is the one to reach for when a UI is watching: the descriptor
fetch happens **inside** the reply loop, so a set that answers in 40 ms surfaces
then instead of after the slowest set in the window has answered. `discover_tvs`
is `discover_tvs_with(|_, _| {})`, so there is one implementation.

# Rules it holds to

- **Silence is an error, not an empty list.** `DiscoveryError::SilentProbe` means
  nothing answered at all, which is a different statement from "no displays on this
  network" — and usually the wrong one to show a person.
- **Dedup is by `USN`, falling back to `LOCATION`.** A single TV answers several
  search targets and must appear once.
- **A missing `LOCATION` or a failed descriptor fetch is skipped silently.** One
  broken responder does not fail a scan.
- **The timeout is the caller's.** `with_timeout`, `with_interface` and
  `with_bind_addr` exist for short windows and multi-homed hosts; the defaults are
  a two-second `MX`.

# What it deliberately does not do

- **It does not identify vendors.** `identify_vendor` lives in `tv-rs`, because it
  is a product decision, not a protocol fact.
- **It does not speak mDNS.** `MDNS_MULTICAST_IP` and the port are declared for the
  planned path; only SSDP is implemented.
- **It does not reach across subnets.** SSDP is multicast and link-local by design.

# Development

- **Tests:** `cargo test -p tv-rs-discovery`. Parsing is covered by fixtures; the
  socket loop is exercised against the real multicast group, so the suite expects a
  host with a network interface.
- **No wrap:** `cargo fmt --all --check` must be clean.
