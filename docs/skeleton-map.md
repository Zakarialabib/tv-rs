# Playbook → code map

Which protocol section became which crate, and what happened to the parts that
did not become code.

| Playbook section | Crate | State |
|---|---|---|
| Discovery (SSDP, UPnP descriptor) | `tv-rs-discovery` | implemented; mDNS and ARP not |
| Power (Wake-on-LAN, probe) | `tv-rs-power` | implemented; WoWLAN reasons not modelled |
| Samsung Tizen (channel, DIAL) | `tv-rs-tizen` | implemented, incl. token store and connection pool |
| LG webOS (SSAP) | `tv-rs-webos` | implemented, incl. pairing and `client-key` persistence |
| Roku (ECP) | `tv-rs-roku` | implemented |
| Media (206, HLS) | `tv-rs-media` | server implemented; segmentation not |
| TLS + WebSocket plumbing | `tv-rs-transport` | shared by both WebSocket vendors |
| Casting (Google Cast, AirPlay) | — | not started; different trust model and payload schemas |
| Android TV (ADB) | — | not started; requires developer mode per device |

## The three layers

```
facade        tv-rs            discovery orchestration, vendor identification, dispatch
protocol      tizen webos roku discovery power     one crate per wire protocol
support       transport media                      TLS/frames, and the HTTP file server
```

A crate may depend on `transport`, never on another protocol crate. The facade is
the only place that knows several vendors exist.
