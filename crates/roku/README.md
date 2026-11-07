# tv-rs-roku

Roku External Control Protocol (ECP) client over HTTP REST (port 8060).

## Supported Operations

- **Device Info**: Model, serial, MAC addresses, network configuration.
- **Power & Mode**: PowerOn, PowerOff, standby queries.
- **Key Press**: Standard navigation keys, media transport keys, home, and volume.
- **App Management**: List installed channels and launch apps with custom parameters.
- **Active App Query**: Fast foreground app status via `/query/active-app`.
