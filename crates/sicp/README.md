# tv-rs-sicp

Philips Serial Information Control Protocol (SICP) over TCP 5000 and RS-232 serial.

## Features

- Binary frame serialization with length header and XOR checksum.
- Multi-display addressing (`DisplayId`).
- Power status and control (On, Off, Standby).
- Input switching (HDMI1, HDMI2, DisplayPort, VGA, DVI, etc.).
- Volume control (0-100).
- Display temperature and signal status queries.
- Implementation of `tv-rs-core::DeviceHandle`.
