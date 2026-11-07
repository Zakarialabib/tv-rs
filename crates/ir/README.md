# tv-rs-ir

Infrared emitter and remote control protocol abstractions for legacy consumer panels and displays.

## Capabilities

- Formats standard NEC, RC-5, and Sony SIRC infrared pulse trains.
- Dispatches commands through connected USB/serial IR blasters or GPIO hardware.
- Implements `tv-rs-core::KeyInput` and `PowerControl` traits.
