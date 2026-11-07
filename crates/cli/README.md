# tv-rs-cli

Command-line reference consumer and diagnostic interface for the `tv-rs` workspace (`publish = false`).

## Commands

- `tv-rs discover`: Sweeps the local network via SSDP and lists detected panels.
- `tv-rs status <ip>`: Inspects power, device info, and supported capabilities.
- `tv-rs wake <ip>`: Transmits Wake-on-LAN packets.
- `tv-rs key <ip> <key>`: Dispatches remote navigation keys.
- `tv-rs launch <ip> <app_id>`: Launches applications or channels.
- `tv-rs ensure <manifest>`: Drives reconciliation against a desired-state file.
