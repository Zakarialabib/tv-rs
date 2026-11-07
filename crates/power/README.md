# tv-rs-power

Wake-on-LAN (WoL) packet transmission and multi-stage TCP/HTTP power probing.

## Features

- **Magic Packet Generator**: Dispatches standard UDP broadcast Wake-on-LAN packets to target MAC addresses.
- **Three-Tier Power Probe**: Probes target IPs over well-known TV ports to classify state as `On`, `Standby`, or `Off` with associated confidence levels.
