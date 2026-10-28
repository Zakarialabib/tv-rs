# tv-rs-lgcmd

LG commercial ASCII serial protocol (ka/mc) over TCP 9761 or RS-232 for the `tv-rs` workspace.

## Protocol

LG's commercial protocol uses 2-character command headers (like `ka` for power, `mc` for keys, `kf` for volume). It was built for RS-232 serial control, but is often exposed over TCP port 9761 on LG commercial displays and set-top boxes.

## Features

- TCP 9761 communication (default)
- RS-232 support (via `serial` feature)
- Power, Volume, Mute, and Remote Key commands
